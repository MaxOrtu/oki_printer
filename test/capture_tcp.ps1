# capture_tcp.ps1 - listener TCP su 127.0.0.1:9100 che salva tutto quello che DF invia
# Uso: powershell -File capture_tcp.ps1   (lasciare aperto mentre si stampa da DF)
param(
    [int]$Port = 9100,
    [string]$Out = "capture.prn"
)

$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $Port)
$listener.Start()
Write-Host "In ascolto su 127.0.0.1:$Port ... stampa qualcosa da DF, io catturo tutto."
Write-Host "(Ctrl+C per terminare)"

while ($true) {
    $client = $listener.AcceptTcpClient()  # blocca finche' non arriva un job
    $remote = $client.Client.RemoteEndPoint
    $stamp = Get-Date -Format "HHmmss"
    $file = "$stamp-$Out"
    Write-Host "[$(Get-Date -Format HH:mm:ss)] Connessione da $remote -> $file"
    $stream = $client.GetStream()
    $fs = [System.IO.File]::Create($file)
    try {
        $stream.CopyTo($fs)
    } finally {
        $fs.Close()
        $client.Close()
    }
    $size = (Get-Item $file).Length
    Write-Host "[$(Get-Date -Format HH:mm:ss)] Catturato: $file ($size byte)"
}
