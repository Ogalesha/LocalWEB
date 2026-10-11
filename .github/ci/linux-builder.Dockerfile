ARG BASE_IMAGE=ubuntu:26.04
FROM ${BASE_IMAGE}
SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
RUN if command -v apt-get >/dev/null; then \
      apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y \
        build-essential autoconf bison re2c pkg-config git curl ca-certificates \
        python3 xz-utils sudo systemd systemd-sysv dbus polkitd libcap2-bin \
        nginx apache2 mariadb-server postgresql-18 postgresql-client-18 libpq-dev \
        redis-server libssl-dev libcurl4-openssl-dev libzip-dev libonig-dev \
        libicu-dev libxml2-dev libsqlite3-dev libpng-dev libjpeg-dev libfreetype-dev \
        libaio1t64 libnuma1 patchelf libx11-dev libxkbcommon-dev libwayland-dev libgl-dev \
        libegl-dev xdg-utils xvfb zstd; \
    else \
      pacman -Syu --noconfirm --needed base-devel autoconf bison re2c pkgconf git \
        curl ca-certificates python xz sudo systemd systemd-sysvcompat dbus polkit libcap \
        nginx apache mariadb mariadb-clients postgresql redis openssl libzip patchelf \
        oniguruma icu libxml2 sqlite libpng libjpeg-turbo freetype2 libaio numactl \
        libx11 libxkbcommon wayland mesa xdg-utils xorg-server-xvfb zstd; \
    fi
RUN if getent passwd 1000 >/dev/null; then userdel "$(getent passwd 1000 | cut -d: -f1)"; fi && \
    useradd --create-home --uid 1000 builder && \
    printf 'builder ALL=(ALL) NOPASSWD: ALL\n' > /etc/sudoers.d/localweb-builder
RUN for unit in nginx apache2 httpd mariadb mysql postgresql redis redis-server; do \
      systemctl disable "$unit.service" 2>/dev/null || true; \
    done
RUN if [ -f /usr/lib/x86_64-linux-gnu/libaio.so.1t64 ]; then \
      ln -sf /usr/lib/x86_64-linux-gnu/libaio.so.1t64 /usr/local/lib/libaio.so.1 && ldconfig; \
    fi
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal && \
    rustup component add rustfmt clippy && \
    chmod -R a+rX /opt/rustup /opt/cargo && chown -R builder:builder /opt/cargo
COPY --chown=builder:builder . /workspace
WORKDIR /workspace
STOPSIGNAL SIGRTMIN+3
CMD ["/sbin/init"]
