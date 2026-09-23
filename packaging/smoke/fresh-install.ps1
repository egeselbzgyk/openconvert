# Row 15.19, the scripted half, on a clean Windows machine: verify the downloaded installer, install
# it for this user, convert a book with the installed app, and check the EPUB it wrote.
#
#   powershell -ExecutionPolicy Bypass -File packaging\smoke\fresh-install.ps1 `
#       -Installer .\OpenConvert_1.0.0_x64-setup.exe -Pdf .\book.pdf [-Sha256 <from the release notes>]
#
# The installer is unsigned in v1 (D12): SmartScreen does not intervene in a scripted install, so the
# person running this also runs the installer by hand once and records what SmartScreen shows. Watch
# the screen while this runs: no console window may flash (D13.2, CREATE_NO_WINDOW) — a script cannot
# see that.
param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$Pdf,
    [string]$Sha256 = ""
)
$ErrorActionPreference = "Stop"

function Fail($message) { Write-Error "fresh-install: $message"; exit 1 }

$Installer = (Resolve-Path $Installer).Path
$Pdf = (Resolve-Path $Pdf).Path
if ($Sha256 -ne "") {
    $actual = (Get-FileHash $Installer -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $Sha256.ToLowerInvariant()) { Fail "SHA-256 mismatch: the release says $Sha256, the file is $actual" }
    Write-Output "fresh-install: SHA-256 matches the release notes"
}
$epub = [System.IO.Path]::ChangeExtension($Pdf, ".epub")
if (Test-Path $epub) { Fail "$epub already exists; the app never overwrites, so the check would be void" }

if ($Installer.EndsWith(".msi")) {
    $process = Start-Process msiexec.exe -ArgumentList "/i", "`"$Installer`"", "/qn" -Wait -PassThru
} else {
    $process = Start-Process $Installer -ArgumentList "/S" -Wait -PassThru
}
if ($process.ExitCode -ne 0) { Fail "the installer exited $($process.ExitCode)" }

$roots = @("$env:LOCALAPPDATA\OpenConvert", "$env:LOCALAPPDATA\Programs\OpenConvert", "$env:ProgramFiles\OpenConvert")
$app = $null
foreach ($root in $roots) {
    if (Test-Path $root) {
        $app = Get-ChildItem $root -Filter *.exe |
            Where-Object { $_.Name -notmatch '^(openconvert-engine|llama-server|uninstall)' } |
            Select-Object -First 1
        if ($null -ne $app) { break }
    }
}
if ($null -eq $app) { Fail "the installed app was not found under $($roots -join ', ')" }

$run = Start-Process $app.FullName -ArgumentList "--smoke-convert", "`"$Pdf`"" -Wait -PassThru
if ($run.ExitCode -ne 0) { Fail "the app exited $($run.ExitCode)" }
if (-not (Test-Path $epub)) { Fail "no EPUB at $epub" }

# An EPUB's first entry is `mimetype`, stored uncompressed, so its content sits at byte 38.
$bytes = [System.IO.File]::ReadAllBytes($epub)
$mimetype = [System.Text.Encoding]::ASCII.GetString($bytes, 38, 20)
if ($mimetype -ne "application/epub+zip") { Fail "$epub is not an EPUB (mimetype: $mimetype)" }
Write-Output "fresh-install: $epub is an EPUB; the installed app converts a book"
