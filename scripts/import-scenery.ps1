# Optional source preparation; ordinary builds use the committed PNG frame sequences.
# Sources: NASA's Goddard Space Flight Center Conceptual Image Lab.
# neutron.webm: https://svs.gsfc.nasa.gov/20267/ (Solo_NS_Prores.webm)
# magnetar-wide.webm: https://svs.gsfc.nasa.gov/14115/ (02_MAGNETAR_Wide_view_BlipFlares_web.webm)
param(
    [Parameter(Mandatory = $true)][string]$Ffmpeg,
    [Parameter(Mandatory = $true)][string]$SourceDirectory,
    [ValidateSet('neutron star', 'magnetar')][string]$Name
)

$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
$destination = Join-Path $repository 'assets/images/ambient'
$clips = @(
    @{ File = 'neutron.webm'; Name = 'neutron star'; Start = 0; Duration = 6; Edge = '0.18'; Key = '0.045' },
    @{ File = 'magnetar-wide.webm'; Name = 'magnetar'; Start = 0; Duration = 10.4; Edge = '0.18'; Key = '0.045' }
)

foreach ($clip in $clips) {
    if ($Name -and $clip.Name -ne $Name) { continue }
    # Normalize each selected interval to seven seconds, then overlap the last second
    # with the first. Playback starts one second into the source, producing a six-second
    # forward-motion loop whose last frame naturally leads back into its first frame.
    # The neutron star uses the steadier opening before the beam angle changes substantially;
    # its 48 frames play over 18 seconds in-game, while the other new loops take six seconds.
    $rate = (7.0 / $clip.Duration).ToString('R', [Globalization.CultureInfo]::InvariantCulture)
    $duration = $clip.Duration.ToString([Globalization.CultureInfo]::InvariantCulture)
    $filter = "[0:v]trim=start=$($clip.Start):duration=${duration},setpts=(PTS-STARTPTS)*${rate}," +
        'scale=384:216:flags=lanczos,fps=8,format=yuv444p,split=2[body][head];' +
        '[body]trim=start=1,setpts=PTS-STARTPTS,fps=8[tail];' +
        '[head]trim=duration=1,setpts=PTS-STARTPTS,fps=8[start];' +
        '[tail][start]xfade=transition=fade:duration=1:offset=5,format=rgba,' +
        "colorkey=0x050508:$($clip.Key):0.2," +
        "geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':" +
        "a='alpha(X,Y)*pow(clip(min(min(X,W-1-X)/W,min(Y,H-1-Y)/H)/$($clip.Edge),0,1),2)'[out]"
    & $Ffmpeg -hide_banner -loglevel error -y -i (Join-Path $SourceDirectory $clip.File) `
        -filter_complex $filter -map '[out]' -an -frames:v 48 -fps_mode passthrough `
        (Join-Path $destination "$($clip.Name) %d.png")
    if ($LASTEXITCODE -ne 0) { throw "Failed to import $($clip.Name)." }
}
