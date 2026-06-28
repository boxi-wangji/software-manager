# Visual target: find text (OCR) or color in a window, move mouse, optional left click.
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RawArgs
)

$ErrorActionPreference = "Stop"
$OutputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$Log = Join-Path $PSScriptRoot "visual-target.log"

function Write-Log([string]$Message) {
    $line = "[{0}] {1}" -f (Get-Date -Format "yyyy-MM-dd HH:mm:ss"), $Message
    Write-Host $line
    try { Add-Content -Path $Log -Value $line -Encoding UTF8 -ErrorAction SilentlyContinue } catch {}
}

function Write-Result($Payload) {
    $json = $Payload | ConvertTo-Json -Compress
    Write-Output ("RESULT_JSON:$json")
}

function Get-ArgValue([string[]]$Names, $DefaultValue) {
    for ($i = 0; $i -lt $RawArgs.Count; $i++) {
        if ($Names -contains $RawArgs[$i]) {
            if ($i + 1 -ge $RawArgs.Count) { throw "Missing value for $($RawArgs[$i])" }
            return $RawArgs[$i + 1]
        }
    }
    return $DefaultValue
}

function Test-ArgFlag([string[]]$Names) {
    foreach ($name in $Names) {
        if ($RawArgs -contains $name) { return $true }
    }
    return $false
}

function Normalize-Text([string]$Text) {
    if ([string]::IsNullOrWhiteSpace($Text)) { return "" }
    return (($Text -replace '\s+', '').ToLowerInvariant())
}

function Convert-HexText([string]$Hex) {
    $chars = New-Object System.Collections.Generic.List[char]
    foreach ($part in ($Hex -split '\s+')) {
        if ([string]::IsNullOrWhiteSpace($part)) { continue }
        $chars.Add([char][Convert]::ToInt32($part, 16))
    }
    return -join $chars
}

function Get-UiMessage([string]$Key) {
    switch ($Key) {
        "preview-moved" { return Convert-HexText "5DF2 9884 89C8 5750 6807 5E76 79FB 52A8 9F20 6807" }
        "preview-move-failed" { return Convert-HexText "5DF2 627E 5230 5750 6807 FF0C 4F46 79FB 52A8 9F20 6807 53EF 80FD 5931 8D25" }
        "moved-clicked" { return Convert-HexText "5DF2 79FB 52A8 5E76 70B9 51FB" }
        "moved-click-failed" { return Convert-HexText "5DF2 79FB 52A8 9F20 6807 FF0C 70B9 51FB 53EF 80FD 5931 8D25" }
        "moved-only" { return Convert-HexText "5DF2 79FB 52A8 9F20 6807" }
        "window-not-found" { return Convert-HexText "672A 627E 5230 76EE 6807 7A97 53E3" }
        "target-not-found" { return Convert-HexText "672A 627E 5230 5339 914D 76EE 6807" }
        default { return $Key }
    }
}

$Win32Source = @"
using System;
using System.Runtime.InteropServices;

public static class VtWin32 {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, System.Text.StringBuilder text, int count);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextLength(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);
    [DllImport("user32.dll", EntryPoint = "SendInput")] public static extern uint SendKeyboardInput(uint nInputs, KEYINPUT[] pInputs, int cbSize);
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int nIndex);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool GetPhysicalCursorPos(out POINT point);
    [DllImport("user32.dll")] public static extern bool SetPhysicalCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr dpiFlag);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr dpiFlag);

    public const int SW_RESTORE = 9;
    public const int SM_XVIRTUALSCREEN = 76;
    public const int SM_YVIRTUALSCREEN = 77;
    public const int SM_CXVIRTUALSCREEN = 78;
    public const int SM_CYVIRTUALSCREEN = 79;
    public const uint INPUT_MOUSE = 0;
    public const uint INPUT_KEYBOARD = 1;
    public const uint MOUSEEVENTF_MOVE = 0x0001;
    public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
    public const uint MOUSEEVENTF_LEFTUP = 0x0004;
    public const uint MOUSEEVENTF_ABSOLUTE = 0x8000;
    public const uint MOUSEEVENTF_VIRTUALDESK = 0x4000;
    public const uint KEYEVENTF_KEYUP = 0x0002;
    public const uint KEYEVENTF_UNICODE = 0x0004;
    public const ushort VK_CONTROL = 0x11;
    public const ushort VK_A = 0x41;

    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT { public int dx, dy; public uint mouseData, dwFlags, time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT { public ushort wVk, wScan; public uint dwFlags, time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Explicit, Size = 40)]
    public struct INPUT {
        [FieldOffset(0)] public uint type;
        [FieldOffset(8)] public MOUSEINPUT mi;
    }
    [StructLayout(LayoutKind.Explicit, Size = 40)]
    public struct KEYINPUT {
        [FieldOffset(0)] public uint type;
        [FieldOffset(8)] public KEYBDINPUT ki;
    }

    public static void FocusWindow(IntPtr hWnd) {
        if (hWnd == IntPtr.Zero) return;
        ShowWindow(hWnd, SW_RESTORE);
        BringWindowToTop(hWnd);
        IntPtr fg = GetForegroundWindow();
        uint dummy;
        uint targetThread = GetWindowThreadProcessId(hWnd, out dummy);
        uint fgThread = GetWindowThreadProcessId(fg, out dummy);
        uint curThread = GetCurrentThreadId();
        bool a = false, b = false;
        if (fgThread != 0 && fgThread != curThread) a = AttachThreadInput(curThread, fgThread, true);
        if (targetThread != 0 && targetThread != curThread) b = AttachThreadInput(curThread, targetThread, true);
        SetForegroundWindow(hWnd);
        if (b) AttachThreadInput(curThread, targetThread, false);
        if (a) AttachThreadInput(curThread, fgThread, false);
    }

    public static bool ClickScreenSendInput(int screenX, int screenY) {
        UseDpiAwareness();
        int vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        int vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        int vw = Math.Max(GetSystemMetrics(SM_CXVIRTUALSCREEN), 1);
        int vh = Math.Max(GetSystemMetrics(SM_CYVIRTUALSCREEN), 1);
        int nx = (int)Math.Round(((screenX - vx) * 65535.0) / Math.Max(vw - 1, 1));
        int ny = (int)Math.Round(((screenY - vy) * 65535.0) / Math.Max(vh - 1, 1));
        uint f = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
        INPUT[] inputs = new INPUT[3];
        inputs[0].type = INPUT_MOUSE; inputs[0].mi.dx = nx; inputs[0].mi.dy = ny; inputs[0].mi.dwFlags = f;
        inputs[1].type = INPUT_MOUSE; inputs[1].mi.dx = nx; inputs[1].mi.dy = ny; inputs[1].mi.dwFlags = f | MOUSEEVENTF_LEFTDOWN;
        inputs[2].type = INPUT_MOUSE; inputs[2].mi.dx = nx; inputs[2].mi.dy = ny; inputs[2].mi.dwFlags = f | MOUSEEVENTF_LEFTUP;
        return SendInput(3, inputs, Marshal.SizeOf(typeof(INPUT))) == 3;
    }

    public static void ClickCurrentPosition() {
        UseDpiAwareness();
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, UIntPtr.Zero);
        System.Threading.Thread.Sleep(50);
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, UIntPtr.Zero);
    }

    static KEYINPUT Key(ushort vk, ushort scan, uint flags) {
        KEYINPUT input = new KEYINPUT();
        input.type = INPUT_KEYBOARD;
        input.ki.wVk = vk;
        input.ki.wScan = scan;
        input.ki.dwFlags = flags;
        return input;
    }

    public static bool SendCtrlA() {
        KEYINPUT[] inputs = new KEYINPUT[] {
            Key(VK_CONTROL, 0, 0),
            Key(VK_A, 0, 0),
            Key(VK_A, 0, KEYEVENTF_KEYUP),
            Key(VK_CONTROL, 0, KEYEVENTF_KEYUP),
        };
        return SendKeyboardInput((uint)inputs.Length, inputs, Marshal.SizeOf(typeof(KEYINPUT))) == inputs.Length;
    }

    public static bool TypeUnicodeText(string text) {
        if (String.IsNullOrEmpty(text)) return true;
        KEYINPUT[] inputs = new KEYINPUT[text.Length * 2];
        int i = 0;
        foreach (char ch in text) {
            inputs[i++] = Key(0, ch, KEYEVENTF_UNICODE);
            inputs[i++] = Key(0, ch, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP);
        }
        return SendKeyboardInput((uint)inputs.Length, inputs, Marshal.SizeOf(typeof(KEYINPUT))) == inputs.Length;
    }

    public static bool MoveScreenSendInput(int screenX, int screenY) {
        UseDpiAwareness();
        int vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        int vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        int vw = Math.Max(GetSystemMetrics(SM_CXVIRTUALSCREEN), 1);
        int vh = Math.Max(GetSystemMetrics(SM_CYVIRTUALSCREEN), 1);
        int nx = (int)Math.Round(((screenX - vx) * 65535.0) / Math.Max(vw - 1, 1));
        int ny = (int)Math.Round(((screenY - vy) * 65535.0) / Math.Max(vh - 1, 1));
        uint f = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
        INPUT[] inputs = new INPUT[1];
        inputs[0].type = INPUT_MOUSE; inputs[0].mi.dx = nx; inputs[0].mi.dy = ny; inputs[0].mi.dwFlags = f;
        return SendInput(1, inputs, Marshal.SizeOf(typeof(INPUT))) == 1;
    }

    public static POINT GetCursorPoint() {
        POINT point;
        GetCursorPos(out point);
        return point;
    }

    public static bool IsCursorNear(int screenX, int screenY, int tolerance) {
        POINT point = GetCursorPoint();
        return Math.Abs(point.X - screenX) <= tolerance && Math.Abs(point.Y - screenY) <= tolerance;
    }

    public static bool IsPhysicalCursorNear(int screenX, int screenY, int tolerance) {
        POINT point;
        if (!GetPhysicalCursorPos(out point)) return false;
        return Math.Abs(point.X - screenX) <= tolerance && Math.Abs(point.Y - screenY) <= tolerance;
    }

    public static bool MoveScreenVerified(int screenX, int screenY) {
        UseDpiAwareness();

        bool physicalOk = SetPhysicalCursorPos(screenX, screenY);
        System.Threading.Thread.Sleep(80);
        if (IsPhysicalCursorNear(screenX, screenY, 6) || IsCursorNear(screenX, screenY, 6)) return true;

        bool directOk = SetCursorPos(screenX, screenY);
        System.Threading.Thread.Sleep(80);
        if (IsPhysicalCursorNear(screenX, screenY, 6) || IsCursorNear(screenX, screenY, 6)) return true;

        bool sendInputOk = MoveScreenSendInput(screenX, screenY);
        System.Threading.Thread.Sleep(80);
        if (IsPhysicalCursorNear(screenX, screenY, 6) || IsCursorNear(screenX, screenY, 6)) return sendInputOk || directOk || physicalOk;

        int vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        int vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        int vw = Math.Max(GetSystemMetrics(SM_CXVIRTUALSCREEN), 1);
        int vh = Math.Max(GetSystemMetrics(SM_CYVIRTUALSCREEN), 1);
        int nx = (int)Math.Round(((screenX - vx) * 65535.0) / Math.Max(vw - 1, 1));
        int ny = (int)Math.Round(((screenY - vy) * 65535.0) / Math.Max(vh - 1, 1));
        uint f = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
        mouse_event(f, (uint)nx, (uint)ny, 0, UIntPtr.Zero);
        System.Threading.Thread.Sleep(80);
        if (IsPhysicalCursorNear(screenX, screenY, 6) || IsCursorNear(screenX, screenY, 6)) return true;

        for (int i = 0; i < 12; i++) {
            POINT point = GetCursorPoint();
            int dx = screenX - point.X;
            int dy = screenY - point.Y;
            if (Math.Abs(dx) <= 6 && Math.Abs(dy) <= 6) return true;
            mouse_event(MOUSEEVENTF_MOVE, unchecked((uint)dx), unchecked((uint)dy), 0, UIntPtr.Zero);
            System.Threading.Thread.Sleep(60);
            if (IsPhysicalCursorNear(screenX, screenY, 6)) return true;
        }

        return IsPhysicalCursorNear(screenX, screenY, 6) || IsCursorNear(screenX, screenY, 6);
    }

    public static string MetricsText() {
        UseDpiAwareness();
        return String.Format(
            "virtual=({0},{1},{2},{3})",
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN)
        );
    }

    public static void UseDpiAwareness() {
        try { SetProcessDpiAwarenessContext(new IntPtr(-4)); } catch {}
        try { SetThreadDpiAwarenessContext(new IntPtr(-4)); } catch {}
        try { SetProcessDPIAware(); } catch {}
    }

    [DllImport("user32.dll")]
    public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
}
"@

function Initialize-Win32 {
    if (-not ([System.Management.Automation.PSTypeName]"VtWin32").Type) {
        Add-Type -TypeDefinition $Win32Source
    }
    [VtWin32]::UseDpiAwareness()
    Add-Type -AssemblyName System.Drawing
    Add-Type -AssemblyName System.Windows.Forms
}

function Initialize-WindowsOcr {
    Add-Type -AssemblyName System.Runtime.WindowsRuntime
    $null = [Windows.Storage.StorageFile, Windows.Storage, ContentType=WindowsRuntime]
    $null = [Windows.Storage.Streams.IRandomAccessStreamWithContentType, Windows.Storage.Streams, ContentType=WindowsRuntime]
    $null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Graphics.Imaging, ContentType=WindowsRuntime]
    $null = [Windows.Graphics.Imaging.SoftwareBitmap, Windows.Graphics.Imaging, ContentType=WindowsRuntime]
    $null = [Windows.Graphics.Imaging.BitmapPixelFormat, Windows.Graphics.Imaging, ContentType=WindowsRuntime]
    $null = [Windows.Graphics.Imaging.BitmapAlphaMode, Windows.Graphics.Imaging, ContentType=WindowsRuntime]
    $null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType=WindowsRuntime]
    $null = [Windows.Media.Ocr.OcrResult, Windows.Foundation, ContentType=WindowsRuntime]
    $null = [Windows.Globalization.Language, Windows.Globalization, ContentType=WindowsRuntime]

    $script:AsTaskGeneric = ([System.WindowsRuntimeSystemExtensions].GetMethods() |
        Where-Object {
            $_.Name -eq "AsTask" -and
            $_.GetParameters().Count -eq 1 -and
            $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
        } |
        Select-Object -First 1)

    if ($null -eq $script:AsTaskGeneric) {
        throw "Failed to load WinRT AsTask bridge."
    }

    $langs = @([Windows.Media.Ocr.OcrEngine]::AvailableRecognizerLanguages)
    $lang = $langs |
        Where-Object { $_.LanguageTag -eq "zh-Hans-CN" -or $_.LanguageTag -eq "zh-CN" -or $_.LanguageTag -match '^zh' } |
        Select-Object -First 1
    if (-not $lang) {
        $available = ($langs | ForEach-Object { $_.LanguageTag }) -join ", "
        throw "Chinese OCR language pack is not installed. Available: $available"
    }
    $script:OcrEngine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromLanguage($lang)
    if (-not $script:OcrEngine) { throw "Failed to create OCR engine ($($lang.LanguageTag))." }
}

function Await-WinRt($Operation, [Type]$ResultType) {
    $asTask = $script:AsTaskGeneric.MakeGenericMethod($ResultType)
    $task = $asTask.Invoke($null, @($Operation))
    try {
        $task.Wait()
    } catch {
        if ($task.Exception -and $task.Exception.InnerException) {
            throw $task.Exception.InnerException
        }
        throw
    }
    if ($task.Exception -and $task.Exception.InnerException) {
        throw $task.Exception.InnerException
    }
    return $task.Result
}

function Get-VisibleWindows {
    $items = New-Object System.Collections.Generic.List[object]
    $cb = [VtWin32+EnumWindowsProc]{
        param($hWnd, $lParam)
        if (-not [VtWin32]::IsWindowVisible($hWnd)) { return $true }
        $len = [VtWin32]::GetWindowTextLength($hWnd)
        if ($len -le 0) { return $true }
        $sb = New-Object System.Text.StringBuilder ($len + 1)
        [void][VtWin32]::GetWindowText($hWnd, $sb, $sb.Capacity)
        $title = $sb.ToString()
        if ([string]::IsNullOrWhiteSpace($title)) { return $true }
        $rect = New-Object "VtWin32+RECT"
        if (-not [VtWin32]::GetWindowRect($hWnd, [ref]$rect)) { return $true }
        $w = $rect.Right - $rect.Left; $h = $rect.Bottom - $rect.Top
        if ($w -lt 80 -or $h -lt 60) { return $true }
        $items.Add([pscustomobject]@{
            Handle = $hWnd; Title = $title
            Left = $rect.Left; Top = $rect.Top; Right = $rect.Right; Bottom = $rect.Bottom
            Width = $w; Height = $h
        })
        return $true
    }
    [void][VtWin32]::EnumWindows($cb, [IntPtr]::Zero)
    return $items
}

function Find-TargetWindow([string]$TitleFilter) {
    $windows = @(Get-VisibleWindows)
    if ($TitleFilter) {
        $filter = $TitleFilter.ToLowerInvariant()
        $match = $windows | Where-Object { $_.Title.ToLowerInvariant().Contains($filter) } |
            Sort-Object { $_.Width * $_.Height } -Descending | Select-Object -First 1
        if ($match) { return $match }
    }
    $fg = [VtWin32]::GetForegroundWindow()
    $fgWin = $windows | Where-Object { $_.Handle -eq $fg } | Select-Object -First 1
    if ($fgWin) { return $fgWin }
    return $windows | Sort-Object { $_.Width * $_.Height } -Descending | Select-Object -First 1
}

function Capture-WindowImage($Window) {
    $path = Join-Path $env:TEMP ("visual-target-{0}.png" -f ([guid]::NewGuid().ToString("N")))
    $bmp = New-Object System.Drawing.Bitmap $Window.Width, $Window.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    try {
        $g.CopyFromScreen($Window.Left, $Window.Top, 0, 0, (New-Object System.Drawing.Size $Window.Width, $Window.Height))
        $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
        return @{ Path = $path; Bitmap = $bmp }
    } catch {
        $g.Dispose(); $bmp.Dispose(); throw
    }
}

function New-PreviewImageDataUrl($Window, $Bitmap, $Point) {
    $clone = New-Object System.Drawing.Bitmap $Bitmap.Width, $Bitmap.Height
    $g = [System.Drawing.Graphics]::FromImage($clone)
    try {
        $g.DrawImage($Bitmap, 0, 0, $Bitmap.Width, $Bitmap.Height)
        $ms = New-Object System.IO.MemoryStream
        try {
            $clone.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
            return "data:image/png;base64," + [Convert]::ToBase64String($ms.ToArray())
        } finally {
            $ms.Dispose()
        }
    } finally {
        $g.Dispose()
        $clone.Dispose()
    }
}

function Read-WindowsOcrImage([string]$Path) {
    $null = [Windows.Storage.StorageFile, Windows.Storage, ContentType=WindowsRuntime]
    $file = Await-WinRt ([Windows.Storage.StorageFile]::GetFileFromPathAsync($Path)) ([Windows.Storage.StorageFile])
    $stream = Await-WinRt ($file.OpenReadAsync()) ([Windows.Storage.Streams.IRandomAccessStreamWithContentType])
    $decoder = Await-WinRt ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
    $bitmap = Await-WinRt ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
    $bitmap = [Windows.Graphics.Imaging.SoftwareBitmap]::Convert($bitmap,
        [Windows.Graphics.Imaging.BitmapPixelFormat]::Bgra8,
        [Windows.Graphics.Imaging.BitmapAlphaMode]::Ignore)
    return Await-WinRt ($script:OcrEngine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])
}

function Get-LineRect($Line) {
    $words = @($Line.Words)
    if ($words.Count -eq 0) { return $null }
    $left = ($words | ForEach-Object { $_.BoundingRect.X } | Measure-Object -Minimum).Minimum
    $top = ($words | ForEach-Object { $_.BoundingRect.Y } | Measure-Object -Minimum).Minimum
    $right = ($words | ForEach-Object { $_.BoundingRect.X + $_.BoundingRect.Width } | Measure-Object -Maximum).Maximum
    $bottom = ($words | ForEach-Object { $_.BoundingRect.Y + $_.BoundingRect.Height } | Measure-Object -Maximum).Maximum
    return [pscustomobject]@{ X = [double]$left; Y = [double]$top; Width = [double]($right - $left); Height = [double]($bottom - $top) }
}

function Find-TextPoint($Window, $OcrResult, [string]$Needle) {
    $normNeedle = Normalize-Text $Needle
    if (-not $normNeedle) { return $null }
    foreach ($line in @($OcrResult.Lines)) {
        $normLine = Normalize-Text $line.Text
        if (-not $normLine.Contains($normNeedle)) { continue }
        $rect = Get-LineRect $line
        if ($null -eq $rect) { continue }
        return [pscustomobject]@{
            ScreenX = [int]($Window.Left + $rect.X + ($rect.Width / 2))
            ScreenY = [int]($Window.Top + $rect.Y + ($rect.Height / 2))
            Detail = $line.Text
        }
    }
    return $null
}

function Parse-Color([string]$Value) {
    $v = $Value.Trim()
    if ($v.StartsWith("#")) { $v = $v.Substring(1) }
    if ($v.Length -eq 6) {
        return [pscustomobject]@{
            R = [Convert]::ToInt32($v.Substring(0, 2), 16)
            G = [Convert]::ToInt32($v.Substring(2, 2), 16)
            B = [Convert]::ToInt32($v.Substring(4, 2), 16)
        }
    }
    throw "Invalid color: $Value (use #RRGGBB)"
}

function Find-ColorPoint($Window, $Bitmap, [int]$R, [int]$G, [int]$B, [int]$Tolerance, [int]$MinPixels) {
    $sumX = 0L; $sumY = 0L; $count = 0
    for ($y = 0; $y -lt $Bitmap.Height; $y++) {
        for ($x = 0; $x -lt $Bitmap.Width; $x++) {
            $c = $Bitmap.GetPixel($x, $y)
            if ([Math]::Abs($c.R - $R) -le $Tolerance -and [Math]::Abs($c.G - $G) -le $Tolerance -and [Math]::Abs($c.B - $B) -le $Tolerance) {
                $sumX += $x; $sumY += $y; $count++
            }
        }
    }
    if ($count -lt $MinPixels) { return $null }
    $cx = [int]($sumX / $count); $cy = [int]($sumY / $count)
    return [pscustomobject]@{
        ScreenX = [int]($Window.Left + $cx)
        ScreenY = [int]($Window.Top + $cy)
        Detail = "color match pixels=$count centroid=($cx,$cy)"
    }
}

function Invoke-PhysicalInput {
    param(
        [int]$ScreenX,
        [int]$ScreenY,
        [bool]$DoClick,
        [string]$WindowTitle,
        [string]$TextToType
    )

    Initialize-Win32
    Write-Log "=== Physical input ==="
    $shouldType = -not [string]::IsNullOrEmpty($TextToType)
    Write-Log ("target=({0},{1}) click={2} type={3}" -f $ScreenX, $ScreenY, $DoClick, $shouldType)
    Write-Log ("metrics: " + [VtWin32]::MetricsText())

    if ($WindowTitle) {
        $window = Find-TargetWindow $WindowTitle
        if ($null -ne $window) {
            Write-Log ("Focus window: {0}" -f $window.Title)
            [VtWin32]::FocusWindow($window.Handle)
            Start-Sleep -Milliseconds 200
        }
    }

    $moved = [VtWin32]::MoveScreenVerified($ScreenX, $ScreenY)
    $clicked = $false
    $typed = $false
    if ($DoClick -or $shouldType) {
        if ($moved) {
            [VtWin32]::ClickCurrentPosition()
            $clicked = $true
            Start-Sleep -Milliseconds 120
        } else {
            Write-Log "Skip click because cursor did not reach target."
        }
    }

    if ($clicked -and $shouldType) {
        [void][VtWin32]::SendCtrlA()
        Start-Sleep -Milliseconds 80
        $typed = [VtWin32]::TypeUnicodeText($TextToType)
        Write-Log ("Typed text length={0} ok={1}" -f $TextToType.Length, $typed)
    }

    $cursor = [VtWin32]::GetCursorPoint()
    Write-Log ("Physical moved=$moved clicked=$clicked typed=$typed cursor=({0},{1})" -f $cursor.X, $cursor.Y)
    Write-Result @{
        success = $(if ($shouldType) { $typed } elseif ($DoClick) { $clicked } else { $moved })
        screen_x = $ScreenX
        screen_y = $ScreenY
        moved = $moved
        clicked = $clicked
        typed = $typed
        cursor_x = $cursor.X
        cursor_y = $cursor.Y
        message = $(if ($shouldType) {
            if ($typed) { "physical-type-ok" } else { "physical-type-failed" }
        } elseif ($DoClick) {
            if ($clicked) { "physical-click-ok" } else { "physical-click-failed" }
        } else {
            if ($moved) { "physical-move-ok" } else { "physical-move-failed" }
        })
    }
    $script:VisualTargetExitCode = 0
}

function Resolve-ManualPoint {
    param($Window, [string]$Value)

    $raw = if ($null -eq $Value) { "" } else { $Value.Trim() }
    if ([string]::IsNullOrWhiteSpace($raw) -or $raw -eq "center") {
        return [pscustomobject]@{
            ScreenX = [int]($Window.Left + ($Window.Width / 2))
            ScreenY = [int]($Window.Top + ($Window.Height / 2))
            Detail = "manual point center"
        }
    }

    $isScreen = $false
    if ($raw.StartsWith("screen:", [System.StringComparison]::OrdinalIgnoreCase)) {
        $isScreen = $true
        $raw = $raw.Substring(7)
    } elseif ($raw.StartsWith("window:", [System.StringComparison]::OrdinalIgnoreCase)) {
        $raw = $raw.Substring(7)
    }

    $parts = @($raw -split "[,\s]+") | Where-Object { $_ -ne "" }
    if ($parts.Count -lt 2) {
        throw "Invalid manual point: $Value"
    }

    $x = [int][double]::Parse($parts[0], [System.Globalization.CultureInfo]::InvariantCulture)
    $y = [int][double]::Parse($parts[1], [System.Globalization.CultureInfo]::InvariantCulture)

    if ($isScreen) {
        return [pscustomobject]@{
            ScreenX = $x
            ScreenY = $y
            Detail = "manual screen point"
        }
    }

    [pscustomobject]@{
        ScreenX = [int]($Window.Left + $x)
        ScreenY = [int]($Window.Top + $y)
        Detail = ("manual window point ({0},{1})" -f $x, $y)
    }
}

function Invoke-VisualTarget {
    $script:VisualTargetExitCode = 0
    $matchType = (Get-ArgValue @("--match-type", "-match-type") "text").ToLowerInvariant()
    $matchValue = Get-ArgValue @("--match-value", "-match-value", "--text", "-text", "--color", "-color") $null
    $windowTitle = Get-ArgValue @("--window-title", "-window-title") ""
    $tolerance = [int](Get-ArgValue @("--tolerance", "-tolerance") 24)
    $minPixels = [int](Get-ArgValue @("--min-pixels", "-min-pixels") 40)
    $offsetX = [int](Get-ArgValue @("--offset-x", "-offset-x") 0)
    $offsetY = [int](Get-ArgValue @("--offset-y", "-offset-y") 0)
    $click = Test-ArgFlag @("--click", "-click")
    $moveOnly = Test-ArgFlag @("--move-only", "-move-only")
    $dryRun = Test-ArgFlag @("--dry-run", "-dry-run")

    if ($matchType -ne "point" -and [string]::IsNullOrWhiteSpace($matchValue)) {
        throw "Missing --match-value (text or #RRGGBB color)."
    }

    Write-Log "=== Visual target started ==="
    Write-Log "match_type=$matchType value=$matchValue window_title=$windowTitle click=$click dry_run=$dryRun"

    Initialize-Win32
    if ($matchType -eq "text") { Initialize-WindowsOcr }

    $window = Find-TargetWindow $windowTitle
    if ($null -eq $window) {
        Write-Result @{ success = $false; message = (Get-UiMessage "window-not-found") }
        $script:VisualTargetExitCode = 1
        return
    }

    Write-Log ("Target window: title='{0}' rect=({1},{2},{3},{4}) size={5}x{6}" -f
        $window.Title, $window.Left, $window.Top, $window.Right, $window.Bottom, $window.Width, $window.Height)

    [VtWin32]::FocusWindow($window.Handle)
    Start-Sleep -Milliseconds 250

    $point = $null
    if ($matchType -eq "point") {
        $point = Resolve-ManualPoint $window $matchValue
    } else {
        $capture = Capture-WindowImage $window
        try {
            if ($matchType -eq "color") {
                $rgb = Parse-Color $matchValue
                $point = Find-ColorPoint $window $capture.Bitmap $rgb.R $rgb.G $rgb.B $tolerance $minPixels
            } else {
                $ocr = Read-WindowsOcrImage $capture.Path
                $point = Find-TextPoint $window $ocr $matchValue
            }
        } finally {
            $capture.Bitmap.Dispose()
            Remove-Item $capture.Path -Force -ErrorAction SilentlyContinue
        }
    }

    if ($null -eq $point) {
        Write-Log "Target not found."
        Write-Result @{ success = $false; message = (Get-UiMessage "target-not-found"); window_title = $window.Title }
        $script:VisualTargetExitCode = 2
        return
    }

    $rawScreenX = $point.ScreenX
    $rawScreenY = $point.ScreenY

    if ($offsetX -ne 0 -or $offsetY -ne 0) {
        $point.ScreenX += $offsetX
        $point.ScreenY += $offsetY
        Write-Log ("Applied offset ({0},{1}) => screen=({2},{3})" -f $offsetX, $offsetY, $point.ScreenX, $point.ScreenY)
    }

    Write-Log ("Found raw=({0},{1}) final=({2},{3}) detail={4}" -f $rawScreenX, $rawScreenY, $point.ScreenX, $point.ScreenY, $point.Detail)

    if ($dryRun) {
        $previewImage = $null
        $previewCapture = Capture-WindowImage $window
        try {
            $previewImage = New-PreviewImageDataUrl $window $previewCapture.Bitmap $point
        } finally {
            $previewCapture.Bitmap.Dispose()
            Remove-Item $previewCapture.Path -Force -ErrorAction SilentlyContinue
        }
        Write-Log ("Preview target resolved at ({0},{1}); clean screenshot generated." -f $point.ScreenX, $point.ScreenY)
        Write-Result @{
            success = $true
            raw_screen_x = $rawScreenX
            raw_screen_y = $rawScreenY
            offset_x = $offsetX
            offset_y = $offsetY
            screen_x = $point.ScreenX
            screen_y = $point.ScreenY
            window_left = $window.Left
            window_top = $window.Top
            window_width = $window.Width
            window_height = $window.Height
            window_title = $window.Title
            window_handle = $window.Handle.ToInt64()
            detail = $point.Detail
            moved = $false
            clicked = $false
            preview_image = $previewImage
            message = "Target resolved."
        }
        $script:VisualTargetExitCode = 0
        return
    }

    Write-Log ("Target resolved at ({0},{1}); host app will handle input." -f $point.ScreenX, $point.ScreenY)

    Write-Result @{
        success = $true
        raw_screen_x = $rawScreenX
        raw_screen_y = $rawScreenY
        offset_x = $offsetX
        offset_y = $offsetY
        screen_x = $point.ScreenX
        screen_y = $point.ScreenY
        window_left = $window.Left
        window_top = $window.Top
        window_width = $window.Width
        window_height = $window.Height
        window_title = $window.Title
        window_handle = $window.Handle.ToInt64()
        detail = $point.Detail
        moved = $false
        clicked = $false
        message = "Target resolved."
    }
    $script:VisualTargetExitCode = 0
}

try {
    $script:VisualTargetExitCode = 0
    $physicalX = Get-ArgValue @("--physical-x") $null
    if ($null -ne $physicalX) {
        $physicalY = [int](Get-ArgValue @("--physical-y") 0)
        $doClick = Test-ArgFlag @("--click", "-click")
        $windowTitle = Get-ArgValue @("--window-title", "-window-title") ""
        $textToType = Get-ArgValue @("--type-text", "-type-text") ""
        Invoke-PhysicalInput -ScreenX ([int]$physicalX) -ScreenY $physicalY -DoClick $doClick -WindowTitle $windowTitle -TextToType $textToType
        exit $script:VisualTargetExitCode
    }
    Invoke-VisualTarget
    exit $script:VisualTargetExitCode
} catch {
    Write-Log ("ERROR: " + $_.Exception.Message)
    Write-Result @{ success = $false; message = $_.Exception.Message }
    exit 9
}
