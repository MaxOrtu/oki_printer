# Crea test_decal.png: quadrato rosso + cerchio bianco su sfondo TRASPARENTE
Add-Type -AssemblyName System.Drawing
$w = 600; $h = 850
$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.Clear([System.Drawing.Color]::Transparent)

# quadrato rosso (in basso a sinistra) - deve uscire con CMY (magenta+giallo)
$red = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255,220,30,30))
$g.FillRectangle($red, 50, 500, 500, 300)

# cerchio bianco esplicito (in alto) - deve uscire col toner BIANCO
$white = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255,255,255,255))
$g.FillEllipse($white, 150, 50, 300, 300)

$g.Dispose()
$bmp.Save("C:\Users\maxor\Documents\Lavori\OR2\REPO\OKI printer\test\test_decal.png", [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "Creata test/test_decal.png (600x850: cerchio bianco sopra, quadrato rosso sotto)"
