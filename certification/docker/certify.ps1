<#
.SYNOPSIS
  Host-side driver for the canonical Occurframe RC3 certification container.

.DESCRIPTION
  Windows is the *host* only. All certification computation happens inside the
  Ubuntu 24.04 x86_64 image defined by certification/docker/Dockerfile.

  The image is built with network access. Every subsequent action runs with
  --network none, which is what makes "the certification runs offline" a
  demonstrated property rather than a claim.

.EXAMPLE
  pwsh -File .\certification\docker\certify.ps1 -Action build
  pwsh -File .\certification\docker\certify.ps1 -Action run
#>
[CmdletBinding()]
param(
    [ValidateSet('build', 'run', 'shell', 'probe')]
    [string]$Action = 'run',

    [string]$Script = 'certification/docker/tasks/next.sh',

    [string]$Tag = 'occurframe-certification:rc3',

    [string]$CorpusPath = '',

    [switch]$AllowNetwork
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot   = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$CorpusRoot = if ($CorpusPath) {
    (Resolve-Path $CorpusPath).Path
} else {
    (Resolve-Path (Join-Path $RepoRoot '..\corpus')).Path
}
$OutRoot    = Join-Path $RepoRoot 'dist\certification'
$LogRoot    = Join-Path $OutRoot 'logs'

New-Item -ItemType Directory -Force -Path $OutRoot, $LogRoot | Out-Null

$LogFile = Join-Path $LogRoot 'last-run.log'
$ToolingSha = (& git -C $RepoRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $ToolingSha -notmatch '^[0-9a-f]{40}$') {
    throw 'Unable to resolve the exact tooling checkout SHA.'
}
$ToolingStatus = (& git -C $RepoRoot status --porcelain=v1 --untracked-files=normal)
if ($LASTEXITCODE -ne 0 -or $ToolingStatus) {
    throw 'Tooling checkout is dirty; HEAD would not identify the certification build context.'
}
$CorpusStatus = (& git -C $CorpusRoot status --porcelain=v1 --untracked-files=normal)
if ($LASTEXITCODE -ne 0 -or $CorpusStatus) {
    throw 'Corpus checkout is dirty; HEAD would not identify the certification authority bytes.'
}

function Write-Log {
    param([string]$Message)
    $line = "[certify] $Message"
    Write-Host $line
    Add-Content -Path $LogFile -Value $line -Encoding utf8
}

function Invoke-Logged {
    param([string[]]$Arguments)
    Write-Log ("docker " + ($Arguments -join ' '))
    & docker @Arguments 2>&1 | Tee-Object -FilePath $LogFile -Append | Out-Host
    $nativeExit = $LASTEXITCODE
    return $nativeExit
}

Set-Content -Path $LogFile -Value "[certify] action=$Action repo=$RepoRoot corpus=$CorpusRoot" -Encoding utf8

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    throw 'docker CLI not found. Start Docker Desktop and reopen the shell.'
}

& docker info --format '{{.ServerVersion}}' | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw 'Docker daemon is not reachable. Start Docker Desktop and wait for it to report Running.'
}

if ($Action -eq 'build') {
    $exit = Invoke-Logged @(
        'build',
        '--file', 'certification/docker/Dockerfile',
        '--tag', $Tag,
        '--progress', 'plain',
        $RepoRoot
    )
    if ($exit -ne 0) { Write-Log "BUILD FAILED ($exit)"; exit $exit }

    $imageId     = (& docker image inspect --format '{{.Id}}' $Tag)
    $baseImage = 'ubuntu:24.04@sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517'
    $baseDigests = 'sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517'
    $provenance = [ordered]@{
        image_reference   = $Tag
        image_id          = $imageId
        base_image        = $baseImage
        base_image_digest = if ($baseDigests) { $baseDigests } else { 'unrecorded' }
    }
    $provenancePath = Join-Path $OutRoot 'image-provenance.json'
    $provenance | ConvertTo-Json -Depth 4 | Set-Content -Path $provenancePath -Encoding utf8
    Write-Log "image built: $imageId"
    Write-Log "image provenance written: $provenancePath"
    exit 0
}

$provenancePath = Join-Path $OutRoot 'image-provenance.json'
$imageId = 'unrecorded'
$baseDigest = 'unrecorded'
if (Test-Path $provenancePath) {
    $recorded = Get-Content $provenancePath -Raw | ConvertFrom-Json
    $imageId = $recorded.image_id
    $baseDigest = $recorded.base_image_digest
}

$network = if ($AllowNetwork) { 'bridge' } else { 'none' }

$runArgs = @(
    'run', '--rm',
    '--network', $network,
    '--mount', "type=bind,source=$RepoRoot,target=/src,readonly",
    '--mount', "type=bind,source=$CorpusRoot,target=/src-corpus,readonly",
    '--mount', "type=bind,source=$OutRoot,target=/out",
    '--mount', 'type=volume,source=occurframe-cargo-target,target=/opt/occurframe/cargo-target',
    '--mount', 'type=volume,source=occurframe-go-cache,target=/root/.cache/go-build',
    '--env', 'OCCURFRAME_REMATERIALISE=1',
    '--env', "OCCURFRAME_IMAGE_REFERENCE=$Tag",
    '--env', "OCCURFRAME_IMAGE_DIGEST=$imageId",
    '--env', "OCCURFRAME_BASE_IMAGE_DIGEST=$baseDigest",
    '--env', "OCCURFRAME_TOOLING_SHA=$ToolingSha",
    # Git 2.35+ rejects a bind-mounted checkout owned by the host UID when the
    # container runs as root. Trust only the fixed, read-only corpus mount; the
    # conformance authority still verifies its cleanliness and exact revision.
    '--env', 'GIT_CONFIG_COUNT=1',
    '--env', 'GIT_CONFIG_KEY_0=safe.directory',
    '--env', 'GIT_CONFIG_VALUE_0=/src-corpus',
    $Tag
)

switch ($Action) {
    'probe' { $runArgs += @('capture-environment') }
    'shell' { $runArgs += @('bash') }
    'run'   { $runArgs += @('bash', "/work/occurframe/$Script") }
}

$exit = Invoke-Logged $runArgs
Write-Log "exit=$exit"
exit $exit
