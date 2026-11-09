param([Parameter(Mandatory=$true)][string]$PayloadDirectory)
$ErrorActionPreference = 'Stop'
$profilePath = Join-Path $env:APPDATA 'com.vladimirkirdan.ssc'
$installPath = Join-Path $env:LOCALAPPDATA 'Programs\SSC'
if ((Test-Path -LiteralPath $profilePath) -or (Test-Path -LiteralPath $installPath)) { throw 'Installer test requires a clean disposable Windows runner' }
$work = Join-Path $env:RUNNER_TEMP ('ssc-installer-smoke-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
try {
    $bootstrap = Join-Path $work 'bootstrap.json'
    $fixture = @{version=1;helper_origin='https://connect.vovakirdan.ru';bootstrap_token=('A' * 43);expires_at=[DateTime]::UtcNow.AddHours(1).ToString('o')}
    [IO.File]::WriteAllText($bootstrap, ($fixture | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
    $installer = Join-Path $work 'setup.exe'
    $compiler = Join-Path ${env:ProgramFiles(x86)} 'NSIS\makensis.exe'
    node scripts/packaging/build-personal.mjs --platform windows --payload $PayloadDirectory --bootstrap $bootstrap --output $installer --compiler $compiler
    if ($LASTEXITCODE -ne 0) { throw 'Fixture installer build failed' }
    $process = Start-Process -FilePath $installer -ArgumentList '/S' -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw 'Fixture installation failed' }
    if ((Get-FileHash (Join-Path $installPath 'ssc.exe')).Hash -ne (Get-FileHash (Join-Path $PayloadDirectory 'ssc.exe')).Hash) { throw 'Installed binary changed' }
    if ((Get-Content -LiteralPath (Join-Path $profilePath 'bootstrap.json') -Raw | ConvertFrom-Json).bootstrap_token -ne $fixture.bootstrap_token) { throw 'Bootstrap transfer failed' }
    $allowed = @([Security.Principal.WindowsIdentity]::GetCurrent().User.Value, 'S-1-5-18')
    foreach ($rule in (Get-Acl -LiteralPath $profilePath).Access) {
        if ($rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -notin $allowed) { throw 'Unexpected profile access rule' }
    }
    [IO.File]::WriteAllText((Join-Path $profilePath 'access.key'), 'installer-test-identity')
    [IO.File]::WriteAllText((Join-Path $profilePath 'access.active'), 'active')
    Remove-Item -LiteralPath (Join-Path $profilePath 'bootstrap.json')
    $process = Start-Process -FilePath $installer -ArgumentList '/S' -Wait -PassThru
    if ($process.ExitCode -ne 0 -or (Test-Path (Join-Path $profilePath 'bootstrap.json')) -or [IO.File]::ReadAllText((Join-Path $profilePath 'access.key')) -ne 'installer-test-identity') { throw 'Update replaced an activated identity' }
    Start-Process -FilePath (Join-Path $installPath 'uninstall.exe') -ArgumentList '/S' -Wait | Out-Null
    for ($attempt=0; $attempt -lt 100 -and (Test-Path (Join-Path $installPath 'ssc.exe')); $attempt++) { Start-Sleep -Milliseconds 100 }
    if ((Test-Path (Join-Path $installPath 'ssc.exe')) -or -not (Test-Path (Join-Path $profilePath 'access.key'))) { throw 'Uninstall did not preserve only the profile' }
    Write-Output 'PASS Windows installation, profile ACL, update identity preservation and uninstall (no activation request sent)'
} finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    # Only this disposable test profile is removed, never an existing installation.
    Remove-Item -LiteralPath $profilePath -Recurse -Force -ErrorAction SilentlyContinue
}
