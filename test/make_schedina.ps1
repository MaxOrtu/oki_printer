# Crea schedina-riferimento toner (da stampare e attaccare sulla stampante)
Add-Type -AssemblyName System.Drawing
$w = 1600; $h = 900
$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.SmoothingMode]::AntiAlias
$g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAlias

$g.Clear([System.Drawing.Color]::White)

$fontTitle = New-Object System.Drawing.Font("Arial Black", 40)
$fontSub   = New-Object System.Drawing.Font("Arial", 22)
$fontLab   = New-Object System.Drawing.Font("Arial Black", 26)
$fontNote  = New-Object System.Drawing.Font("Arial", 16)

$black = [System.Drawing.Brushes]::Black
$g.DrawString("OKI C711WT - POSIZIONE TONER", $fontTitle, $black, 40, 20)

$colors = @{
    "W" = [System.Drawing.Color]::FromArgb(248, 248, 248)
    "K" = [System.Drawing.Color]::FromArgb(20, 20, 20)
    "Y" = [System.Drawing.Color]::FromArgb(255, 210, 0)
    "M" = [System.Drawing.Color]::FromArgb(230, 0, 140)
    "C" = [System.Drawing.Color]::FromArgb(0, 155, 220)
}

function DrawRig($y, $rigTitle, $c1name, $c1, $c2name, $c2, $c3name, $c3, $c4name, $c4, $border1) {
    $g.DrawString($rigTitle, $fontSub, $black, 40, $y)
    $x = 60; $bw = 300; $bh = 140
    foreach ($c in @(@($c1name,$c1,$border1), @($c2name,$c2,$false), @($c3name,$c3,$false), @($c4name,$c4,$false))) {
        $brush = New-Object System.Drawing.SolidBrush $c[1]
        $g.FillRectangle($brush, $x, ($y + 45), $bw, $bh)
        if ($c[2]) {
            $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(90,90,90)), 3
            $g.DrawRectangle($pen, $x, ($y + 45), $bw, $bh)
        }
        $labelColor = if ($c[1].GetBrightness() -gt 0.6) { [System.Drawing.Brushes]::Black } else { [System.Drawing.Brushes]::White }
        $g.DrawString($c[0], $fontLab, $labelColor, ($x + 20), ($y + 85))
        $x += $bw + 60
    }
}

DrawRig 130 "RIG DECALS  ( decals / bianco sotto )" "BIANCO" $colors["W"] "GIALLO" $colors["Y"] "MAGENTA" $colors["M"] "CIANO" $colors["C"] $true

$g.DrawString("scambia SOLO il primo: bianco con nero", $fontNote, $black, 40, 335)

DrawRig 380 "RIG FOTO/DOC  ( foto CMYK / nero vero )" "NERO" $colors["K"] "GIALLO" $colors["Y"] "MAGENTA" $colors["M"] "CIANO" $colors["C"] $true

$g.DrawString("1 = deposita per primo (sotto)     4 = deposita per ultimo (sopra)", $fontNote, $black, 40, 620)
$g.DrawString("GIALLO / MAGENTA / CIANO non si muovono MAI", $fontNote, $black, 40, 660)
$g.DrawString("APP: Queue = Decals (bianco)  |  Queue = Photo + K toner (nero)", $fontNote, $black, 40, 700)

$g.Dispose()
$bmp.Save("C:\Users\maxor\Documents\Lavori\OR2\REPO\OKI printer\test\schedina_toner.png", [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "Creata test/schedina_toner.png"
