# Send-RAW.ps - invia un file RAW (es. PostScript) direttamente a una stampante Windows
# Uso: powershell -File Send-RAW.ps -PrinterName "OKI C711WT" -File .\test_mapping.ps
param(
    [Parameter(Mandatory=$true)][string]$PrinterName,
    [Parameter(Mandatory=$true)][string]$File
)

$code = @'
using System;
using System.Runtime.InteropServices;
public class RawPrinter {
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Ansi)]
    public struct DOCINFOA {
        [MarshalAs(UnmanagedType.LPStr)] public string docName;
        [MarshalAs(UnmanagedType.LPStr)] public string outputFile;
        [MarshalAs(UnmanagedType.LPStr)] public string dataType;
    }
    [DllImport("winspool.Drv", EntryPoint="OpenPrinterA", SetLastError=true, CharSet=CharSet.Ansi)]
    public static extern bool OpenPrinter([In]string szPrinter, out IntPtr hPrinter, IntPtr pd);
    [DllImport("winspool.Drv", EntryPoint="ClosePrinter", SetLastError=true)]
    public static extern bool ClosePrinter(IntPtr hPrinter);
    [DllImport("winspool.Drv", EntryPoint="StartDocPrinterA", SetLastError=true, CharSet=CharSet.Ansi)]
    public static extern bool StartDocPrinter(IntPtr hPrinter, Int32 level, [In] ref DOCINFOA di);
    [DllImport("winspool.Drv", EntryPoint="EndDocPrinter", SetLastError=true)]
    public static extern bool EndDocPrinter(IntPtr hPrinter);
    [DllImport("winspool.Drv", EntryPoint="StartPagePrinter", SetLastError=true)]
    public static extern bool StartPagePrinter(IntPtr hPrinter);
    [DllImport("winspool.Drv", EntryPoint="EndPagePrinter", SetLastError=true)]
    public static extern bool EndPagePrinter(IntPtr hPrinter);
    [DllImport("winspool.Drv", EntryPoint="WritePrinter", SetLastError=true)]
    public static extern bool WritePrinter(IntPtr hPrinter, IntPtr pBytes, Int32 dwCount, out Int32 dwWritten);
    public static bool SendBytes(string printer, byte[] bytes) {
        IntPtr h; if (!OpenPrinter(printer, out h, IntPtr.Zero)) return false;
        var di = new DOCINFOA { docName = "OKI Decal Test", dataType = "RAW" };
        bool ok = StartDocPrinter(h, 1, ref di);
        if (ok) {
            StartPagePrinter(h);
            IntPtr p = Marshal.AllocCoTaskMem(bytes.Length);
            Marshal.Copy(bytes, 0, p, bytes.Length);
            int written; ok = WritePrinter(h, p, bytes.Length, out written);
            Marshal.FreeCoTaskMem(p);
            EndPagePrinter(h);
        }
        EndDocPrinter(h); ClosePrinter(h);
        return ok;
    }
}
'@
Add-Type -TypeDefinition $code

$bytes = [System.IO.File]::ReadAllBytes($File)
if ([RawPrinter]::SendBytes($PrinterName, $bytes)) {
    Write-Host "OK: $($bytes.Length) byte inviati a '$PrinterName'"
} else {
    Write-Error "Invio fallito a '$PrinterName' (err: $([System.Runtime.InteropServices.Marshal]::GetLastWin32Error()))"
}
