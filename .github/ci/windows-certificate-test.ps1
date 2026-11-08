# The Windows user-root store asks for confirmation even with certutil -f.
# Accept only the generated test CA, in the native certutil process. The
# application keeps that system confirmation for interactive users.
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
public static class LocalWebTestConsent {
    private delegate bool EnumWindow(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumChildWindows(IntPtr window, EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] private static extern IntPtr GetDlgItem(IntPtr window, int id);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    public static void AcceptExactCertificate(string thumbprint) {
        EnumWindows((window, parameter) => {
            uint pid;
            GetWindowThreadProcessId(window, out pid);
            try {
                using (var process = Process.GetProcessById((int)pid)) {
                    if (!String.Equals(process.MainModule.FileName,
                        System.IO.Path.Combine(Environment.SystemDirectory, "certutil.exe"),
                        StringComparison.OrdinalIgnoreCase)) return true;
                }
            } catch { return true; }
            var text = new StringBuilder();
            EnumChildWindows(window, (child, unused) => {
                var part = new StringBuilder(8192);
                GetWindowText(child, part, part.Capacity);
                text.Append(part);
                return true;
            }, IntPtr.Zero);
            var normalized = Regex.Replace(text.ToString(), @"[\s:]", "").ToUpperInvariant();
            if (normalized.Contains(thumbprint)) {
                var yes = GetDlgItem(window, 6); // IDYES
                if (yes != IntPtr.Zero) PostMessage(window, 0x0111, new IntPtr(6), yes); // WM_COMMAND, IDYES
            }
            return true;
        }, IntPtr.Zero);
    }
}
'@

$stdout = Join-Path $env:RUNNER_TEMP 'localweb-cert-test.stdout'
$stderr = Join-Path $env:RUNNER_TEMP 'localweb-cert-test.stderr'
$process = Start-Process cargo -PassThru -NoNewWindow -RedirectStandardOutput $stdout -RedirectStandardError $stderr -ArgumentList @(
    'test', '-p', 'localweb-core', 'windows_current_user_ca_install_query_and_remove',
    '--', '--ignored', '--test-threads=1', '--nocapture'
)
$null = $process.Handle
$timer = [Diagnostics.Stopwatch]::StartNew()
try {
    while (-not $process.HasExited) {
        if ($timer.Elapsed.TotalSeconds -gt 180) {
            & (Join-Path ([Environment]::SystemDirectory) 'taskkill.exe') /PID $process.Id /T /F
            throw 'The Windows CA integration test exceeded three minutes.'
        }
        $output = Get-Content $stdout -Raw -ErrorAction SilentlyContinue
        if ($output -match 'LocalWeb test CA thumbprint: ([A-F0-9]{40})') {
            [LocalWebTestConsent]::AcceptExactCertificate($Matches[1])
        }
        Start-Sleep -Milliseconds 200
        $process.Refresh()
    }
    $process.WaitForExit()
    $result = $process.ExitCode
    if ($null -eq $result) { throw 'Could not read the certificate test exit code.' }
} finally {
    Get-Content $stdout, $stderr | Tee-Object -FilePath (Join-Path $env:RUNNER_TEMP 'localweb-tests.log') -Append
    $process.Dispose()
}
exit $result
