# Test GDI tramite il driver Windows: 4 rettangoli (rosso, verde, blu, nero)
Add-Type -AssemblyName System.Drawing

$printerName = "OKI C711WT"
$pd = New-Object System.Drawing.Printing.PrintDocument
$pd.PrinterSettings.PrinterName = $printerName

# pagina A4
$pd.DefaultPageSettings.Landscape = $false

$page = 0
$pd.add_PrintPage({
    param($sender, $e)
    $g = $e.Graphics
    $script:page++
    # 4 rettangoli: rosso, verde, blu, nero (RGB puri -> il driver li separa in CMYK)
    $colors = @(
        @(255,0,0),    # rosso   -> M+Y
        @(0,255,0),    # verde   -> C+Y
        @(0,0,255),    # blu     -> C+M
        @(0,0,0)       # nero    -> K
    )
    $y = 100
    foreach ($c in $colors) {
        $brush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb($c[0],$c[1],$c[2]))
        $g.FillRectangle($brush, 150, $y, 400, 150)
        $y += 220
    }
    $font = New-Object System.Drawing.Font("Arial", 14)
    $black = [System.Drawing.Brushes]::Black
    $g.DrawString("RIGHE: rosso / verde / blu / nero", $font, $black, 150, 30)
    $e.HasMorePages = $false
})

$pd.Print()
Write-Host "Inviata pagina di test GDI a '$printerName'"
