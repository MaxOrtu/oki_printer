# Crea test_white_text.png: scritta bianca GROSSA su sfondo blu scuro (opaco)
Add-Type -AssemblyName System.Drawing
$w = 1600; $h = 500
$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.Clear([System.Drawing.Color]::FromArgb(255, 20, 40, 130))  # blu scuro opaco
$g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAlias

# font grosso e grasso
$font = New-Object System.Drawing.Font("Arial Black", 140, [System.Drawing.FontStyle]::Bold)
$white = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)
$fmt = New-Object System.Drawing.StringFormat
$fmt.Alignment = [System.Drawing.StringAlignment]::Center
$fmt.LineAlignment = [System.Drawing.StringAlignment]::Center
$rect = New-Object System.Drawing.RectangleF(0, 0, $w, $h)
$g.DrawString("DECAL OK!", $font, $white, $rect, $fmt)

$g.Dispose()
$bmp.Save("C:\Users\maxor\Documents\Lavori\OR2\REPO\OKI printer\test\test_white_text.png", [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "Creata test/test_white_text.png (1600x500: 'DECAL OK!' bianca su blu)"
