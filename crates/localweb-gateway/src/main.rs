use anyhow::{Context, Result};
use bytes::Bytes;
use clap::Parser;
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::{
    Method, Request, Response, StatusCode, Uri, body::Incoming, header::HOST, server::conn::http1,
    service::service_fn,
};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::{TokioExecutor, TokioIo},
};
use localweb_core::{GatewayRoute, Paths, certificate::CaPaths, validate_domain};
use rustls::{ServerConfig, server::ResolvesServerCertUsingSni, sign::CertifiedKey};
use std::{
    collections::HashMap, convert::Infallible, fs, io::BufReader, net::SocketAddr, sync::Arc,
};
use tokio::{
    net::TcpListener,
    time::{Duration, timeout},
};
use tokio_rustls::TlsAcceptor;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

type GatewayBody = BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;
type HttpClient = Client<HttpConnector, Incoming>;

/// How long a client may take to complete the TLS handshake before being dropped.
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long an upstream project server may take to answer a proxied request.
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 443)]
    https_port: u16,
}

#[derive(Clone)]
struct App {
    routes: Arc<HashMap<String, GatewayRoute>>,
    client: HttpClient,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let paths = Paths::discover()?;
    paths.create()?;
    let routes = load_routes(&paths)?;
    let config = tls_config(&paths, &routes)?;
    let app = App {
        routes: Arc::new(routes),
        client: Client::builder(TokioExecutor::new()).build_http(),
    };
    let https = https_listener(args.https_port).await?;
    let tls = TlsAcceptor::from(Arc::new(config));
    loop {
        let (stream, peer) = https.accept().await?;
        let app = app.clone();
        let tls = tls.clone();
        tokio::spawn(async move {
            if let Ok(Ok(stream)) = timeout(TLS_HANDSHAKE_TIMEOUT, tls.accept(stream)).await {
                let service = service_fn(move |request| handle(request, app.clone(), peer));
                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .with_upgrades()
                    .await;
            }
        });
    }
}

async fn https_listener(port: u16) -> Result<TcpListener> {
    #[cfg(target_os = "macos")]
    if let Some(listener) = launchd_listener()? {
        return Ok(listener);
    }
    TcpListener::bind(("127.0.0.1", port))
        .await
        .context("bind HTTPS loopback port")
}

/// Takes the launchd-owned socket when the gateway runs as a LaunchAgent.
///
/// Binding port 443 as an unprivileged user is only possible through launchd
/// socket activation; when the process was started manually this returns
/// `None` so a plain bind can be attempted instead.
#[cfg(target_os = "macos")]
fn launchd_listener() -> Result<Option<TcpListener>> {
    use std::{ffi::CStr, os::fd::FromRawFd, ptr};

    unsafe extern "C" {
        fn launch_activate_socket(
            name: *const std::ffi::c_char,
            descriptors: *mut *mut libc::c_int,
            count: *mut libc::size_t,
        ) -> libc::c_int;
    }

    let name = CStr::from_bytes_with_nul(b"LocalWebHTTPS\0").expect("static socket name");
    let mut descriptors = ptr::null_mut();
    let mut count: libc::size_t = 0;
    // SAFETY: both out-pointers are valid for the duration of the call; launchd
    // allocates the descriptor array and reports its length.
    let result =
        unsafe { launch_activate_socket(name.as_ptr(), &raw mut descriptors, &raw mut count) };
    if result != 0 || count == 0 || descriptors.is_null() {
        return Ok(None);
    }
    // SAFETY: launchd returned an array of `count` descriptors.
    let descriptor = unsafe { *descriptors };
    for index in 1..count {
        // SAFETY: `index` is within the array launchd allocated.
        unsafe {
            libc::close(*descriptors.add(index));
        }
    }
    // SAFETY: launchd allocated the array with malloc and passes ownership.
    unsafe {
        libc::free(descriptors.cast());
    }
    // SAFETY: the activated descriptor is now owned by this process.
    let listener = unsafe { std::net::TcpListener::from_raw_fd(descriptor) };
    listener.set_nonblocking(true)?;
    Ok(Some(TcpListener::from_std(listener)?))
}

fn load_routes(paths: &Paths) -> Result<HashMap<String, GatewayRoute>> {
    let file = paths.runtime.join("gateway-routes.json");
    if !file.exists() {
        return Ok(HashMap::new());
    }
    let routes: Vec<GatewayRoute> = serde_json::from_slice(&fs::read(file)?)?;
    let mut map = HashMap::new();
    for route in routes {
        validate_domain(&route.hostname)?;
        if map.insert(route.hostname.clone(), route).is_some() {
            anyhow::bail!("duplicate gateway hostname");
        }
    }
    Ok(map)
}

fn tls_config(paths: &Paths, routes: &HashMap<String, GatewayRoute>) -> Result<ServerConfig> {
    let ca = CaPaths::new(paths);
    let mut resolver = ResolvesServerCertUsingSni::new();
    for hostname in routes.keys() {
        let cert = ca.leaves.join(format!("{hostname}.crt"));
        let key = ca.leaves.join(format!("{hostname}.key"));
        let mut cert_reader = BufReader::new(fs::File::open(cert)?);
        let chain = rustls_pemfile::certs(&mut cert_reader).collect::<std::io::Result<Vec<_>>>()?;
        let mut key_reader = BufReader::new(fs::File::open(key)?);
        let private_key =
            rustls_pemfile::private_key(&mut key_reader)?.context("missing leaf private key")?;
        let provider = rustls::crypto::ring::default_provider();
        let certified = CertifiedKey::from_der(chain, private_key, &provider)?;
        resolver.add(hostname, certified)?;
    }
    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(resolver));
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

async fn handle(
    request: Request<Incoming>,
    app: App,
    peer: SocketAddr,
) -> Result<Response<GatewayBody>, Infallible> {
    Ok(route_request(request, app, peer).await)
}

async fn route_request(
    mut request: Request<Incoming>,
    app: App,
    peer: SocketAddr,
) -> Response<GatewayBody> {
    let hostname = request
        .headers()
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let Some(route) = app.routes.get(&hostname) else {
        return page(
            StatusCode::NOT_FOUND,
            "Unknown LocalWeb project",
            "No LocalWeb project owns this hostname.",
        );
    };
    let Some(port) = route.upstream_port else {
        return page(
            StatusCode::SERVICE_UNAVAILABLE,
            "Project stopped",
            "Start this project with `localweb project start`.",
        );
    };
    if request.method() == Method::CONNECT {
        return page(
            StatusCode::METHOD_NOT_ALLOWED,
            "Unsupported method",
            "CONNECT is unavailable.",
        );
    }
    let original_host = request.headers().get(HOST).cloned();
    let path = request
        .uri()
        .path_and_query()
        .map_or("/", hyper::http::uri::PathAndQuery::as_str);
    let uri = Uri::builder()
        .scheme("http")
        .authority(format!("127.0.0.1:{port}"))
        .path_and_query(path)
        .build();
    let Ok(uri) = uri else {
        return page(
            StatusCode::BAD_REQUEST,
            "Invalid URL",
            "The request target is invalid.",
        );
    };
    *request.uri_mut() = uri;
    if let Some(host) = original_host {
        request.headers_mut().insert("x-forwarded-host", host);
    }
    if let Ok(value) = "https".parse() {
        request.headers_mut().insert("x-forwarded-proto", value);
    }
    if let Ok(value) = peer.ip().to_string().parse() {
        request.headers_mut().insert("x-forwarded-for", value);
    }
    match timeout(UPSTREAM_TIMEOUT, app.client.request(request)).await {
        Ok(Ok(response)) => response.map(|body| {
            body.map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
                .boxed()
        }),
        Ok(Err(_)) => page(
            StatusCode::BAD_GATEWAY,
            "LocalWeb upstream error",
            "The project web server did not respond.",
        ),
        Err(_) => page(
            StatusCode::GATEWAY_TIMEOUT,
            "LocalWeb timeout",
            "The project web server took too long to respond.",
        ),
    }
}

fn page(status: StatusCode, heading: &str, detail: &str) -> Response<GatewayBody> {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>LocalWeb</title><h1>{heading}</h1><p>{detail}</p>"
    );
    Response::builder()
        .status(status)
        .header("content-type", "text/html; charset=utf-8")
        .body(
            Full::new(Bytes::from(body))
                .map_err(|never| match never {})
                .boxed(),
        )
        .expect("static response")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn error_page_status_is_real() {
        assert_eq!(
            page(StatusCode::SERVICE_UNAVAILABLE, "Stopped", "Start it").status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}
