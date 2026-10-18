# Mesure le WorkingSet64/Private de dillo.exe au repos (fenêtre ouverte ~6 s).
$p = Start-Process -FilePath "$PSScriptRoot\..\target\release\dillo.exe" -PassThru
Start-Sleep -Seconds 6
$ws = [math]::Round($p.WorkingSet64 / 1MB, 1)
$pm = [math]::Round($p.PrivateMemorySize64 / 1MB, 1)
Write-Output "PID $($p.Id) WS=${ws}MB Private=${pm}MB Responding=$($p.Responding)"
Stop-Process -Id $p.Id -Force
