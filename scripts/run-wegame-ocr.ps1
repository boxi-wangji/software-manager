# WeGame Windows OCR installer-window scanner.
# Uses Windows.Media.Ocr and Win32 mouse APIs. No Python dependencies.
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RawArgs
)

$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
$Log = Join-Path $PSScriptRoot "ocr-install.log"

function Write-Log([string]$Message) {
    $line = "[{0}] {1}" -f (Get-Date -Format "yyyy-MM-dd HH:mm:ss"), $Message
    Write-Host $line
    try {
        Add-Content -Path $Log -Value $line -Encoding UTF8 -ErrorAction Stop
    } catch {
        # Console output is enough if the log is locked.
    }
}

function Pause-Exit([int]$Code) {
    Write-Host ""
    Write-Host "Exit code: $Code"
    Read-Host "Press Enter to close this window"
    exit $Code
}

function Get-ArgValue([string[]]$Names, [object]$DefaultValue) {
    for ($i = 0; $i -lt $RawArgs.Count; $i++) {
        if ($Names -contains $RawArgs[$i]) {
            if ($i + 1 -ge $RawArgs.Count) {
                throw "Missing value for $($RawArgs[$i])"
            }
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

function New-UString([int[]]$Codepoints) {
    return -join ($Codepoints | ForEach-Object { [char]$_ })
}

$ClickTargets = @(
    [pscustomobject]@{ Text = (New-UString @(0x540c, 0x610f, 0x5e76, 0x5b89, 0x88c5)); Name = "agree-install"; Priority = 50 },
    [pscustomobject]@{ Text = (New-UString @(0x7acb, 0x5373, 0x5b89, 0x88c5)); Name = "install-now"; Priority = 40 },
    [pscustomobject]@{ Text = (New-UString @(0x5f00, 0x59cb, 0x5b89, 0x88c5)); Name = "start-install"; Priority = 30 },
    [pscustomobject]@{ Text = (New-UString @(0x7ee7, 0x7eed, 0x5b89, 0x88c5)); Name = "continue-install"; Priority = 20 },
    [pscustomobject]@{ Text = (New-UString @(0x5b8c, 0x6210, 0x5b89, 0x88c5)); Name = "finish-install"; Priority = 10 }
)
$InstallText = New-UString @(0x5b89, 0x88c5)
$TencentText = New-UString @(0x817e, 0x8baf)
$AgreeText = New-UString @(0x540c, 0x610f)
$ReadText = New-UString @(0x5df2, 0x9605, 0x8bfb)
$ReadVerbText = New-UString @(0x9605, 0x8bfb)
$UserAgreementText = New-UString @(0x7528, 0x6237, 0x534f, 0x8bae)
$AgreementText = New-UString @(0x534f, 0x8bae)
$PrivacyText = New-UString @(0x9690, 0x79c1)
$CustomInstallText = New-UString @(0x81ea, 0x5b9a, 0x4e49, 0x5b89, 0x88c5)

$Win32Source = @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class SmWin32 {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowTextLength(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int X, int Y);

    [DllImport("user32.dll")]
    public static extern bool GetCursorPos(out POINT point);

    [DllImport("user32.dll")]
    public static extern IntPtr WindowFromPoint(POINT point);

    [DllImport("user32.dll")]
    public static extern bool ScreenToClient(IntPtr hWnd, ref POINT point);

    [DllImport("user32.dll")]
    public static extern IntPtr SendMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("kernel32.dll")]
    public static extern uint GetCurrentThreadId();

    [DllImport("user32.dll")]
    public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);

    [DllImport("user32.dll")]
    public static extern bool BringWindowToTop(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

    [DllImport("user32.dll")]
    public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);

    [DllImport("user32.dll")]
    public static extern int GetSystemMetrics(int nIndex);

    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();

    [DllImport("user32.dll")]
    public static extern bool SetProcessDpiAwarenessContext(IntPtr dpiFlag);

    public const int SW_RESTORE = 9;
    public const int SM_XVIRTUALSCREEN = 76;
    public const int SM_YVIRTUALSCREEN = 77;
    public const int SM_CXVIRTUALSCREEN = 78;
    public const int SM_CYVIRTUALSCREEN = 79;
    public const uint INPUT_MOUSE = 0;
    public const uint MOUSEEVENTF_MOVE = 0x0001;
    public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
    public const uint MOUSEEVENTF_LEFTUP = 0x0004;
    public const uint MOUSEEVENTF_ABSOLUTE = 0x8000;
    public const uint MOUSEEVENTF_VIRTUALDESK = 0x4000;
    public const uint WM_MOUSEMOVE = 0x0200;
    public const uint WM_LBUTTONDOWN = 0x0201;
    public const uint WM_LBUTTONUP = 0x0202;
    public const int MK_LBUTTON = 0x0001;

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct POINT {
        public int X;
        public int Y;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MOUSEINPUT {
        public int dx;
        public int dy;
        public uint mouseData;
        public uint dwFlags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct INPUT {
        public uint type;
        public MOUSEINPUT mi;
    }

    public static void FocusWindow(IntPtr hWnd) {
        if (hWnd == IntPtr.Zero) { return; }
        ShowWindow(hWnd, SW_RESTORE);
        BringWindowToTop(hWnd);

        IntPtr fg = GetForegroundWindow();
        if (fg == hWnd) {
            SetForegroundWindow(hWnd);
            return;
        }

        uint dummy;
        uint targetThread = GetWindowThreadProcessId(hWnd, out dummy);
        uint fgThread = GetWindowThreadProcessId(fg, out dummy);
        uint curThread = GetCurrentThreadId();
        bool attachedFg = false;
        bool attachedTarget = false;

        if (fgThread != 0 && fgThread != curThread) {
            attachedFg = AttachThreadInput(curThread, fgThread, true);
        }
        if (targetThread != 0 && targetThread != curThread) {
            attachedTarget = AttachThreadInput(curThread, targetThread, true);
        }

        SetForegroundWindow(hWnd);

        if (attachedTarget) { AttachThreadInput(curThread, targetThread, false); }
        if (attachedFg) { AttachThreadInput(curThread, fgThread, false); }
    }

    public static bool ClickScreenSendInput(int screenX, int screenY) {
        int vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        int vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        int vw = Math.Max(GetSystemMetrics(SM_CXVIRTUALSCREEN), 1);
        int vh = Math.Max(GetSystemMetrics(SM_CYVIRTUALSCREEN), 1);
        int nx = (int)Math.Round(((screenX - vx) * 65535.0) / Math.Max(vw - 1, 1));
        int ny = (int)Math.Round(((screenY - vy) * 65535.0) / Math.Max(vh - 1, 1));
        uint moveFlags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;

        INPUT[] inputs = new INPUT[3];
        inputs[0].type = INPUT_MOUSE;
        inputs[0].mi.dx = nx;
        inputs[0].mi.dy = ny;
        inputs[0].mi.dwFlags = moveFlags;

        inputs[1].type = INPUT_MOUSE;
        inputs[1].mi.dx = nx;
        inputs[1].mi.dy = ny;
        inputs[1].mi.dwFlags = moveFlags | MOUSEEVENTF_LEFTDOWN;

        inputs[2].type = INPUT_MOUSE;
        inputs[2].mi.dx = nx;
        inputs[2].mi.dy = ny;
        inputs[2].mi.dwFlags = moveFlags | MOUSEEVENTF_LEFTUP;

        return SendInput(3, inputs, Marshal.SizeOf(typeof(INPUT))) == 3;
    }
}
"@

function Initialize-Win32 {
    Add-Type -AssemblyName System.Drawing
    if (-not ([System.Management.Automation.PSTypeName]"SmWin32").Type) {
        Add-Type -TypeDefinition $Win32Source
    }
    try { [void][SmWin32]::SetProcessDpiAwarenessContext([IntPtr](-4)) } catch {}
    try { [void][SmWin32]::SetProcessDPIAware() } catch {}
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
        throw "Cannot load WinRT async bridge."
    }

    $languages = @([Windows.Media.Ocr.OcrEngine]::AvailableRecognizerLanguages)
    $language = $languages |
        Where-Object { $_.LanguageTag -eq "zh-Hans-CN" -or $_.LanguageTag -eq "zh-CN" -or $_.LanguageTag -eq "zh-Hans" } |
        Select-Object -First 1

    if ($null -eq $language) {
        $available = ($languages | ForEach-Object { $_.LanguageTag }) -join ", "
        throw "Windows Simplified Chinese OCR is not installed. Available OCR languages: $available"
    }

    $script:OcrEngine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromLanguage($language)
    if ($null -eq $script:OcrEngine) {
        throw "Failed to create Windows OCR engine for $($language.LanguageTag)."
    }

    Write-Log "Windows OCR language: $($language.LanguageTag)"
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

function Normalize-Text([string]$Text) {
    if ($null -eq $Text) { return "" }
    return (($Text -replace "\s+", "")).ToLowerInvariant()
}

function Match-Target([string]$Text) {
    $norm = Normalize-Text $Text
    if ($norm.Length -eq 0) { return $null }

    foreach ($target in $ClickTargets) {
        $needle = Normalize-Text $target.Text
        if ($norm.Contains($needle) -or $needle.Contains($norm)) {
            return $target
        }
    }
    return $null
}

function Test-AgreementLine([string]$Text) {
    $norm = Normalize-Text $Text
    if ($norm.Length -eq 0) { return $false }

    $hasRead = $norm.Contains((Normalize-Text $ReadText)) -or $norm.Contains((Normalize-Text $ReadVerbText))
    $hasAgree = $norm.Contains((Normalize-Text $AgreeText))
    $hasAgreement = $norm.Contains((Normalize-Text $UserAgreementText)) -or $norm.Contains((Normalize-Text $AgreementText))
    $hasPrivacy = $norm.Contains((Normalize-Text $PrivacyText))
    $hasWeGame = $norm.Contains("wegame")

    return ($hasRead -and ($hasAgree -or $hasAgreement -or $hasPrivacy)) -or
        ($hasAgree -and ($hasAgreement -or $hasPrivacy -or $hasWeGame))
}

function Test-CustomInstallLine([string]$Text) {
    $norm = Normalize-Text $Text
    if ($norm.Length -eq 0) { return $false }
    return $norm.Contains((Normalize-Text $CustomInstallText))
}

function Get-VisibleWindows {
    $items = New-Object System.Collections.Generic.List[object]
    $callback = [SmWin32+EnumWindowsProc]{
        param([IntPtr]$hWnd, [IntPtr]$lParam)

        if (-not [SmWin32]::IsWindowVisible($hWnd)) { return $true }

        $windowProcessId = [uint32]0
        [void][SmWin32]::GetWindowThreadProcessId($hWnd, [ref]$windowProcessId)

        $length = [Math]::Max([SmWin32]::GetWindowTextLength($hWnd) + 1, 256)
        $titleBuilder = New-Object System.Text.StringBuilder $length
        [void][SmWin32]::GetWindowText($hWnd, $titleBuilder, $titleBuilder.Capacity)
        $title = $titleBuilder.ToString()

        $rect = New-Object "SmWin32+RECT"
        if (-not [SmWin32]::GetWindowRect($hWnd, [ref]$rect)) { return $true }

        $width = $rect.Right - $rect.Left
        $height = $rect.Bottom - $rect.Top
        if ($width -lt 120 -or $height -lt 80) { return $true }

        $items.Add([pscustomobject]@{
            Handle = $hWnd
            ProcessId = [int]$windowProcessId
            Title = $title
            Left = $rect.Left
            Top = $rect.Top
            Right = $rect.Right
            Bottom = $rect.Bottom
            Width = $width
            Height = $height
        })
        return $true
    }

    [void][SmWin32]::EnumWindows($callback, [IntPtr]::Zero)
    return $items
}

function Find-WeGameWindow([int[]]$KnownPids) {
    $best = $null
    $bestScore = -1

    foreach ($window in (Get-VisibleWindows)) {
        $processName = ""
        try {
            $process = Get-Process -Id $window.ProcessId -ErrorAction SilentlyContinue
            if ($process) { $processName = $process.ProcessName }
        } catch {}

        $haystack = ("{0} {1}" -f $window.Title, $processName).ToLowerInvariant()
        $score = -1

        if ($KnownPids -contains $window.ProcessId) { $score += 1000000 }
        if ($haystack.Contains("wegame")) { $score += 100000 }
        if ($haystack.Contains("tgp") -or $haystack.Contains("miniloader") -or $haystack.Contains("tencent")) { $score += 50000 }
        if ($window.Title.Contains($InstallText)) { $score += 20000 }
        if ($window.Title.Contains($TencentText)) { $score += 10000 }

        if ($score -ge 0) {
            $score += [int](($window.Width * $window.Height) / 1000)
            if ($score -gt $bestScore) {
                $best = $window
                $bestScore = $score
            }
        }
    }

    return $best
}

function Capture-WindowImage($Window) {
    $path = Join-Path $env:TEMP ("wegame-window-{0}.png" -f ([guid]::NewGuid().ToString("N")))
    $bitmap = New-Object System.Drawing.Bitmap $Window.Width, $Window.Height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $size = New-Object System.Drawing.Size $Window.Width, $Window.Height
        $graphics.CopyFromScreen($Window.Left, $Window.Top, 0, 0, $size)
        $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
        return $path
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

function Read-WindowsOcrImage([string]$Path) {
    $stream = $null
    $bitmap = $null
    try {
        $file = Await-WinRt ([Windows.Storage.StorageFile]::GetFileFromPathAsync($Path)) ([Windows.Storage.StorageFile])
        $stream = Await-WinRt ($file.OpenReadAsync()) ([Windows.Storage.Streams.IRandomAccessStreamWithContentType])
        $decoder = Await-WinRt ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
        $bitmap = Await-WinRt ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
        $bitmap = [Windows.Graphics.Imaging.SoftwareBitmap]::Convert(
            $bitmap,
            [Windows.Graphics.Imaging.BitmapPixelFormat]::Bgra8,
            [Windows.Graphics.Imaging.BitmapAlphaMode]::Ignore
        )
        return (Await-WinRt ($script:OcrEngine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult]))
    } finally {
        if ($bitmap) { $bitmap.Dispose() }
        if ($stream) { $stream.Dispose() }
    }
}

function Get-LineRect($Line) {
    $words = @($Line.Words)
    if ($words.Count -eq 0) { return $null }

    $left = ($words | ForEach-Object { $_.BoundingRect.X } | Measure-Object -Minimum).Minimum
    $top = ($words | ForEach-Object { $_.BoundingRect.Y } | Measure-Object -Minimum).Minimum
    $right = ($words | ForEach-Object { $_.BoundingRect.X + $_.BoundingRect.Width } | Measure-Object -Maximum).Maximum
    $bottom = ($words | ForEach-Object { $_.BoundingRect.Y + $_.BoundingRect.Height } | Measure-Object -Maximum).Maximum

    return [pscustomobject]@{
        X = [double]$left
        Y = [double]$top
        Width = [double]($right - $left)
        Height = [double]($bottom - $top)
    }
}

function New-MouseLParam([int]$X, [int]$Y) {
    $value = (($Y -band 0xffff) -shl 16) -bor ($X -band 0xffff)
    return [IntPtr]$value
}

function Invoke-MouseMessageClick($Hwnd, [int]$ScreenX, [int]$ScreenY, [bool]$UsePostMessage) {
    if ($Hwnd -eq [IntPtr]::Zero) { return $false }

    $clientPoint = New-Object "SmWin32+POINT"
    $clientPoint.X = $ScreenX
    $clientPoint.Y = $ScreenY
    if (-not [SmWin32]::ScreenToClient($Hwnd, [ref]$clientPoint)) {
        Write-Log ("ScreenToClient failed for hwnd={0} at ({1}, {2})." -f $Hwnd, $ScreenX, $ScreenY)
        return $false
    }

    $lParam = New-MouseLParam $clientPoint.X $clientPoint.Y
    $kind = if ($UsePostMessage) { "PostMessage" } else { "SendMessage" }
    Write-Log ("{0} click hwnd={1} screen=({2}, {3}) client=({4}, {5})" -f $kind, $Hwnd, $ScreenX, $ScreenY, $clientPoint.X, $clientPoint.Y)

    if ($UsePostMessage) {
        [void][SmWin32]::PostMessage($Hwnd, [SmWin32]::WM_MOUSEMOVE, [IntPtr]::Zero, $lParam)
        Start-Sleep -Milliseconds 40
        [void][SmWin32]::PostMessage($Hwnd, [SmWin32]::WM_LBUTTONDOWN, [IntPtr][SmWin32]::MK_LBUTTON, $lParam)
        Start-Sleep -Milliseconds 80
        [void][SmWin32]::PostMessage($Hwnd, [SmWin32]::WM_LBUTTONUP, [IntPtr]::Zero, $lParam)
    } else {
        [void][SmWin32]::SendMessage($Hwnd, [SmWin32]::WM_MOUSEMOVE, [IntPtr]::Zero, $lParam)
        Start-Sleep -Milliseconds 40
        [void][SmWin32]::SendMessage($Hwnd, [SmWin32]::WM_LBUTTONDOWN, [IntPtr][SmWin32]::MK_LBUTTON, $lParam)
        Start-Sleep -Milliseconds 80
        [void][SmWin32]::SendMessage($Hwnd, [SmWin32]::WM_LBUTTONUP, [IntPtr]::Zero, $lParam)
    }
    return $true
}

function Click-AtPoint($Window, [int]$X, [int]$Y, [bool]$DryRun) {
    if ($DryRun) { return $true }

    if ($null -ne $Window) {
        [SmWin32]::FocusWindow($Window.Handle)
        Start-Sleep -Milliseconds 200
    }

    if ([SmWin32]::ClickScreenSendInput($X, $Y)) {
        Write-Log ("SendInput click at ({0}, {1})." -f $X, $Y)
        Start-Sleep -Milliseconds 120
        return $true
    }

    Write-Log ("SendInput failed at ({0}, {1}); trying message click." -f $X, $Y)

    $screenPoint = New-Object "SmWin32+POINT"
    $screenPoint.X = $X
    $screenPoint.Y = $Y
    $hitHwnd = [SmWin32]::WindowFromPoint($screenPoint)

    if ($hitHwnd -ne [IntPtr]::Zero) {
        if (Invoke-MouseMessageClick $hitHwnd $X $Y $false) { return $true }
        if (Invoke-MouseMessageClick $hitHwnd $X $Y $true) { return $true }
    } else {
        Write-Log ("WindowFromPoint found no target at ({0}, {1})." -f $X, $Y)
    }

    if ($null -ne $Window) {
        if (Invoke-MouseMessageClick $Window.Handle $X $Y $false) { return $true }
        if (Invoke-MouseMessageClick $Window.Handle $X $Y $true) { return $true }
    }

    return (Click-ScreenPointPhysical $X $Y)
}

function Click-ScreenPointPhysical([int]$X, [int]$Y) {
    Write-Log ("Physical mouse fallback for ({0}, {1})." -f $X, $Y)

    [void][SmWin32]::SetCursorPos($X, $Y)
    Start-Sleep -Milliseconds 40
    $point = New-Object "SmWin32+POINT"
    $moved = $false
    if ([SmWin32]::GetCursorPos([ref]$point)) {
        Write-Log ("Mouse positioned at ({0}, {1}) for requested ({2}, {3})" -f $point.X, $point.Y, $X, $Y)
        $moved = ([Math]::Abs($point.X - $X) -le 8 -and [Math]::Abs($point.Y - $Y) -le 8)
    }

    if (-not $moved) {
        $vx = [SmWin32]::GetSystemMetrics([SmWin32]::SM_XVIRTUALSCREEN)
        $vy = [SmWin32]::GetSystemMetrics([SmWin32]::SM_YVIRTUALSCREEN)
        $vw = [Math]::Max([SmWin32]::GetSystemMetrics([SmWin32]::SM_CXVIRTUALSCREEN), 1)
        $vh = [Math]::Max([SmWin32]::GetSystemMetrics([SmWin32]::SM_CYVIRTUALSCREEN), 1)
        $nx = [uint32][Math]::Round((($X - $vx) * 65535.0) / [Math]::Max($vw - 1, 1))
        $ny = [uint32][Math]::Round((($Y - $vy) * 65535.0) / [Math]::Max($vh - 1, 1))
        Write-Log ("SetCursorPos mismatch; trying absolute mouse move nx={0} ny={1} virtual=({2},{3},{4},{5})" -f $nx, $ny, $vx, $vy, $vw, $vh)
        [SmWin32]::mouse_event(
            [SmWin32]::MOUSEEVENTF_MOVE -bor [SmWin32]::MOUSEEVENTF_ABSOLUTE -bor [SmWin32]::MOUSEEVENTF_VIRTUALDESK,
            $nx,
            $ny,
            0,
            [UIntPtr]::Zero
        )
        Start-Sleep -Milliseconds 80
    }

    Start-Sleep -Milliseconds 80
    [SmWin32]::mouse_event([SmWin32]::MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 50
    [SmWin32]::mouse_event([SmWin32]::MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
    return $true
}

function Get-PrepareTargets($Window, $Result) {
    $agreementLine = $null
    $customLine = $null
    $smallBoxes = New-Object System.Collections.Generic.List[object]

    foreach ($line in @($Result.Lines)) {
        $rect = Get-LineRect $line
        if ($null -eq $rect) { continue }

        if ($null -eq $agreementLine -and (Test-AgreementLine $line.Text)) {
            $agreementLine = [pscustomobject]@{ Line = $line; Rect = $rect }
        }

        if ($null -eq $customLine -and (Test-CustomInstallLine $line.Text)) {
            $customLine = [pscustomobject]@{ Line = $line; Rect = $rect }
        }

        if ($rect.Width -le 44 -and $rect.Height -le 44) {
            $smallBoxes.Add([pscustomobject]@{ Line = $line; Rect = $rect })
        }
    }

    $checkbox = $null
    if ($null -ne $agreementLine) {
        $agreementRect = $agreementLine.Rect
        $agreementCenterY = $agreementRect.Y + ($agreementRect.Height / 2)
        $bestBox = $null
        $bestDistance = [double]::MaxValue

        foreach ($box in $smallBoxes) {
            $boxRect = $box.Rect
            $boxCenterY = $boxRect.Y + ($boxRect.Height / 2)
            $verticalDistance = [Math]::Abs($boxCenterY - $agreementCenterY)
            $rightEdge = $boxRect.X + $boxRect.Width
            $horizontalDistance = $agreementRect.X - $rightEdge

            if ($verticalDistance -le 18 -and $horizontalDistance -ge -8 -and $horizontalDistance -le 90) {
                if ($horizontalDistance -lt $bestDistance) {
                    $bestDistance = $horizontalDistance
                    $bestBox = $box
                }
            }
        }

        if ($null -ne $bestBox) {
            $rect = $bestBox.Rect
            $checkbox = [pscustomobject]@{
                Text = $bestBox.Line.Text
                X = [int]($Window.Left + $rect.X + ($rect.Width / 2))
                Y = [int]($Window.Top + $rect.Y + ($rect.Height / 2))
            }
        } else {
            $checkbox = [pscustomobject]@{
                Text = $agreementLine.Line.Text
                X = [int]([Math]::Max($Window.Left + 8, $Window.Left + $agreementRect.X - 14))
                Y = [int]($Window.Top + $agreementRect.Y + ($agreementRect.Height / 2))
            }
        }
    }

    $custom = $null
    if ($null -ne $customLine) {
        $rect = $customLine.Rect
        $custom = [pscustomobject]@{
            Text = $customLine.Line.Text
            X = [int]([Math]::Min($Window.Right - 18, $Window.Left + $rect.X + $rect.Width + 10))
            Y = [int]($Window.Top + $rect.Y + ($rect.Height / 2))
        }
    }

    return [pscustomobject]@{
        Checkbox = $checkbox
        Custom = $custom
    }
}

function Test-PathLine([string]$Text) {
    $norm = Normalize-Text $Text
    if ($norm.Length -eq 0) { return $false }
    if ($norm -match '[a-zA-Z]:\\') { return $true }
    if ($norm.Contains("wegame") -or $norm.Contains("programfiles")) { return $true }
    return $false
}

function Get-PathFieldTarget($Window, $Result) {
    $best = $null
    $bestScore = -1

    foreach ($line in @($Result.Lines)) {
        if ((Test-AgreementLine $line.Text) -or (Test-CustomInstallLine $line.Text)) { continue }
        if (-not (Test-PathLine $line.Text)) { continue }
        $rect = Get-LineRect $line
        if ($null -eq $rect) { continue }

        $score = $rect.Width
        if ($line.Text -match '[a-zA-Z]:\\') { $score += 500 }
        if ($score -gt $bestScore) {
            $bestScore = $score
            $best = [pscustomobject]@{
                Text = $line.Text
                X = [int]($Window.Left + $rect.X + ($rect.Width / 2))
                Y = [int]($Window.Top + $rect.Y + ($rect.Height / 2))
            }
        }
    }

    return $best
}

function Set-InstallPathForWindow($Window, [string]$InstallDir, [bool]$DryRun) {
    if ([string]::IsNullOrWhiteSpace($InstallDir)) { return }

    if ($DryRun) {
        Write-Log "DRY-RUN would set install path to: $InstallDir"
        return
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    [SmWin32]::FocusWindow($Window.Handle)
    Start-Sleep -Milliseconds 400

    $imagePath = Capture-WindowImage $Window
    try {
        $result = Read-WindowsOcrImage $imagePath
        $pathTarget = Get-PathFieldTarget $Window $result
        if ($null -ne $pathTarget) {
            Write-Log ("PREP click path field at ({0}, {1}) from OCR text: {2}" -f $pathTarget.X, $pathTarget.Y, $pathTarget.Text)
            Click-AtPoint $Window $pathTarget.X $pathTarget.Y $false | Out-Null
        } else {
            $fx = [int]($Window.Left + ($Window.Width * 0.35))
            $fy = [int]($Window.Top + ($Window.Height * 0.58))
            Write-Log ("PREP path field not found by OCR; clicking fallback at ({0}, {1})" -f $fx, $fy)
            Click-AtPoint $Window $fx $fy $false | Out-Null
        }
    } finally {
        Remove-Item $imagePath -Force -ErrorAction SilentlyContinue
    }

    Start-Sleep -Milliseconds 300
    [void][SmWin32]::SetForegroundWindow($Window.Handle)
    Start-Sleep -Milliseconds 200

    Set-Clipboard -Value $InstallDir
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.SendKeys]::SendWait("^a")
    Start-Sleep -Milliseconds 100
    [System.Windows.Forms.SendKeys]::SendWait("^v")
    Start-Sleep -Milliseconds 400
    Write-Log "PREP pasted install path: $InstallDir"
}

function Invoke-PrepareCustomInstall([int[]]$KnownPids, [bool]$DryRun, [string]$InstallDir) {
    $start = Get-Date
    $window = $null

    while (((Get-Date) - $start).TotalSeconds -lt 60) {
        $window = Find-WeGameWindow $KnownPids
        if ($null -ne $window) { break }
        Start-Sleep -Milliseconds 800
    }

    if ($null -eq $window) {
        Write-Log "PREP no WeGame installer window found."
        return 1
    }

    [void][SmWin32]::ShowWindow($window.Handle, [SmWin32]::SW_RESTORE)
    [void][SmWin32]::SetForegroundWindow($window.Handle)
    Start-Sleep -Milliseconds 300

    $imagePath = Capture-WindowImage $window
    try {
        $result = Read-WindowsOcrImage $imagePath
        $targets = Get-PrepareTargets $window $result

        if ($null -ne $targets.Checkbox) {
            Write-Log ("PREP click agreement-checkbox at ({0}, {1}) from OCR text: {2}" -f $targets.Checkbox.X, $targets.Checkbox.Y, $targets.Checkbox.Text)
            [void](Click-AtPoint $window $targets.Checkbox.X $targets.Checkbox.Y $DryRun)
            Start-Sleep -Milliseconds 600
        } else {
            Write-Log "PREP agreement checkbox not found."
        }

        if ($null -ne $targets.Custom) {
            Write-Log ("PREP click custom-install at ({0}, {1}) from OCR text: {2}" -f $targets.Custom.X, $targets.Custom.Y, $targets.Custom.Text)
            [void](Click-AtPoint $window $targets.Custom.X $targets.Custom.Y $DryRun)
            Start-Sleep -Seconds 2

            if (-not [string]::IsNullOrWhiteSpace($InstallDir)) {
                $window = Find-WeGameWindow $KnownPids
                if ($null -ne $window) {
                    Set-InstallPathForWindow $window $InstallDir $DryRun
                } else {
                    Write-Log "PREP could not refocus installer window to set path."
                }
            }
        } else {
            Write-Log "PREP custom-install entry not found."
        }

        return 0
    } finally {
        Remove-Item $imagePath -Force -ErrorAction SilentlyContinue
    }
}

function Scan-And-Click([int[]]$KnownPids, [bool]$DryRun, [bool]$PreferAgreement) {
    $window = Find-WeGameWindow $KnownPids
    if ($null -eq $window) {
        return [pscustomobject]@{ State = "no-window"; Target = $null }
    }

    [void][SmWin32]::ShowWindow($window.Handle, [SmWin32]::SW_RESTORE)
    [void][SmWin32]::SetForegroundWindow($window.Handle)
    Start-Sleep -Milliseconds 250

    $imagePath = Capture-WindowImage $window
    try {
        $result = Read-WindowsOcrImage $imagePath
        $best = $null
        $bestScore = -1
        $agreement = $null

        foreach ($line in @($result.Lines)) {
            $rect = Get-LineRect $line
            if ($null -eq $rect) { continue }

            if ($null -eq $agreement -and (Test-AgreementLine $line.Text)) {
                $agreement = [pscustomobject]@{
                    Text = $line.Text
                    X = [int]([Math]::Max($window.Left + 8, $window.Left + $rect.X - 24))
                    Y = [int]($window.Top + $rect.Y + ($rect.Height / 2))
                }
            }

            $target = Match-Target $line.Text
            if ($null -eq $target) { continue }

            $score = $target.Priority
            if ($score -gt $bestScore) {
                $best = [pscustomobject]@{
                    Target = $target
                    Text = $line.Text
                    X = [int]($window.Left + $rect.X + ($rect.Width / 2))
                    Y = [int]($window.Top + $rect.Y + ($rect.Height / 2))
                }
                $bestScore = $score
            }
        }

        if ($PreferAgreement) {
            if ($null -ne $agreement) {
                Write-Log ("Click agreement-checkbox at ({0}, {1}) from OCR text: {2}" -f $agreement.X, $agreement.Y, $agreement.Text)
                [void](Click-AtPoint $window $agreement.X $agreement.Y $DryRun)
                return [pscustomobject]@{
                    State = "clicked"
                    Target = "agreement-checkbox"
                    X = $agreement.X
                    Y = $agreement.Y
                }
            }

            Write-Log "Agreement checkbox line not recognized; waiting instead of repeating the install button."
            return [pscustomobject]@{ State = "no-agreement"; Target = $null }
        }

        if ($null -eq $best) {
            return [pscustomobject]@{ State = "no-target"; Target = $null }
        }

        Write-Log ("Click {0} at ({1}, {2}) from OCR text: {3}" -f $best.Target.Name, $best.X, $best.Y, $best.Text)
        [void](Click-AtPoint $window $best.X $best.Y $DryRun)
        return [pscustomobject]@{
            State = "clicked"
            Target = $best.Target.Name
            X = $best.X
            Y = $best.Y
        }
    } finally {
        Remove-Item $imagePath -Force -ErrorAction SilentlyContinue
    }
}

function Write-OcrSnapshot([int[]]$KnownPids, [int]$TimeoutSeconds) {
    $start = Get-Date
    $window = $null

    while (((Get-Date) - $start).TotalSeconds -lt $TimeoutSeconds) {
        $window = Find-WeGameWindow $KnownPids
        if ($null -ne $window) { break }
        Start-Sleep -Milliseconds 800
    }

    if ($null -eq $window) {
        Write-Log "SCAN no WeGame installer window found."
        return 1
    }

    [void][SmWin32]::ShowWindow($window.Handle, [SmWin32]::SW_RESTORE)
    [void][SmWin32]::SetForegroundWindow($window.Handle)
    Start-Sleep -Milliseconds 300

    $imagePath = Capture-WindowImage $window
    try {
        $result = Read-WindowsOcrImage $imagePath
        Write-Log ("SCAN window pid={0} title='{1}' rect=({2},{3},{4},{5}) size={6}x{7}" -f
            $window.ProcessId, $window.Title, $window.Left, $window.Top, $window.Right, $window.Bottom, $window.Width, $window.Height)
        Write-Log ("SCAN full_text: {0}" -f $result.Text)

        $index = 0
        foreach ($line in @($result.Lines)) {
            $rect = Get-LineRect $line
            if ($null -eq $rect) { continue }

            $match = "none"
            $target = Match-Target $line.Text
            if ($null -ne $target) {
                $match = $target.Name
            } elseif (Test-AgreementLine $line.Text) {
                $match = "agreement-line"
            } elseif (Test-CustomInstallLine $line.Text) {
                $match = "custom-install"
            }

            $screenX = [int]($window.Left + $rect.X)
            $screenY = [int]($window.Top + $rect.Y)
            $screenW = [int]$rect.Width
            $screenH = [int]$rect.Height
            Write-Log ("SCAN line[{0}] match={1} rect=({2},{3},{4},{5}) text='{6}'" -f
                $index, $match, $screenX, $screenY, $screenW, $screenH, $line.Text)
            $index++
        }

        if ($index -eq 0) {
            Write-Log "SCAN no OCR lines recognized."
        }

        Write-Log "SCAN done. No clicks were sent."
        return 0
    } finally {
        Remove-Item $imagePath -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-SelfTest {
    $path = Join-Path $env:TEMP ("wegame-ocr-selftest-{0}.png" -f ([guid]::NewGuid().ToString("N")))
    $bitmap = New-Object System.Drawing.Bitmap 520, 160
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $font = New-Object System.Drawing.Font "Microsoft YaHei UI", 48, ([System.Drawing.FontStyle]::Bold)
    try {
        $graphics.Clear([System.Drawing.Color]::White)
        $graphics.DrawString($ClickTargets[1].Text, $font, [System.Drawing.Brushes]::Black, 40, 40)
        $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
        $result = Read-WindowsOcrImage $path
        Write-Log "Self-test OCR text: $($result.Text)"
        if ((Normalize-Text $result.Text).Contains((Normalize-Text $ClickTargets[1].Text))) {
            $windowCount = @((Get-VisibleWindows)).Count
            Write-Log "Self-test visible windows: $windowCount"
            Write-Log "Self-test OK."
            return
        }
        throw "Self-test did not recognize the expected text."
    } finally {
        $font.Dispose()
        $graphics.Dispose()
        $bitmap.Dispose()
        Remove-Item $path -Force -ErrorAction SilentlyContinue
    }
}

function Main {
    $installer = Get-ArgValue @("--installer", "-installer", "-Installer") $null
    $timeout = [int](Get-ArgValue @("--timeout", "-timeout", "-Timeout") 900)
    $dryRun = Test-ArgFlag @("--dry-run", "-dry-run", "-DryRun")
    $noLaunch = Test-ArgFlag @("--no-launch", "-no-launch", "-NoLaunch")
    $selfTest = Test-ArgFlag @("--self-test", "-self-test", "-SelfTest")
    $autoClick = Test-ArgFlag @("--auto-click", "-auto-click", "-AutoClick")
    $silentNsis = Test-ArgFlag @("--silent-nsis", "-silent-nsis", "-SilentNsis")
    $installDir = Get-ArgValue @("--install-dir", "-install-dir", "-InstallDir") $null

    Write-Log "=== Windows OCR install started ==="
    Write-Log "Script dir: $PSScriptRoot"

    Initialize-Win32
    Initialize-WindowsOcr

    if ($selfTest) {
        Invoke-SelfTest
        return 0
    }

    if ($silentNsis) {
        if ([string]::IsNullOrWhiteSpace($installer)) {
            throw "Missing --installer path."
        }
        if (-not (Test-Path $installer -PathType Leaf)) {
            throw "Installer not found: $installer"
        }

        $arguments = New-Object System.Collections.Generic.List[string]
        $arguments.Add("/S")
        if (-not [string]::IsNullOrWhiteSpace($installDir)) {
            if ($installDir -match "\s") {
                Write-Log "Install dir contains spaces; NSIS /D path is omitted: $installDir"
            } else {
                if (-not (Test-Path $installDir -PathType Container)) {
                    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
                }
                $arguments.Add("/D=$installDir")
            }
        }

        Write-Log "Launching NSIS silent installer: $installer"
        $proc = Start-Process -FilePath $installer -ArgumentList $arguments.ToArray() -WorkingDirectory (Split-Path $installer -Parent) -PassThru -Wait -WindowStyle Hidden
        $exitCode = if ($null -ne $proc) { [int]$proc.ExitCode } else { 0 }
        Write-Log "NSIS silent installer exited with code $exitCode."
        return $exitCode
    }

    $knownPids = New-Object System.Collections.Generic.List[int]

    if (-not $noLaunch) {
        if ([string]::IsNullOrWhiteSpace($installer)) {
            throw "Missing --installer path."
        }
        if (-not (Test-Path $installer -PathType Leaf)) {
            throw "Installer not found: $installer"
        }

        Write-Log "Launching installer: $installer"
        $proc = Start-Process -FilePath $installer -WorkingDirectory (Split-Path $installer -Parent) -PassThru -WindowStyle Hidden
        if ($proc) { $knownPids.Add([int]$proc.Id) }
        Start-Sleep -Seconds 3
    }

    if (-not $autoClick) {
        Write-Log "Prepare-custom scan mode. It will only click agreement checkbox and custom-install, then scan."
        $prepCode = Invoke-PrepareCustomInstall ($knownPids.ToArray()) $dryRun $installDir
        if ($prepCode -ne 0) {
            Write-Log "PREP did not complete; scanning the current window anyway."
        }
        return (Write-OcrSnapshot ($knownPids.ToArray()) 60)
    }

    Write-Log "Monitoring installer window (Windows OCR)."
    if (-not [string]::IsNullOrWhiteSpace($installDir)) {
        Write-Log "Target install dir: $installDir"
    }
    $prepCode = Invoke-PrepareCustomInstall ($knownPids.ToArray()) $dryRun $installDir
    if ($prepCode -ne 0) {
        Write-Log "PREP did not complete; continuing with OCR auto-click."
    }
    Start-Sleep -Seconds 1

    $start = Get-Date
    $idleRounds = 0
    $seenWindow = $false
    $lastClick = $null
    $lastClickSignature = ""
    $repeatInstallClicks = 0
    $preferAgreement = $false

    while (((Get-Date) - $start).TotalSeconds -lt $timeout) {
        $scan = Scan-And-Click ($knownPids.ToArray()) $dryRun $preferAgreement

        if ($scan.State -eq "clicked") {
            $lastClick = Get-Date
            $idleRounds = 0
            $seenWindow = $true

            if ($scan.Target -eq "agreement-checkbox") {
                $preferAgreement = $false
                $repeatInstallClicks = 0
                $lastClickSignature = ""
                Start-Sleep -Seconds 1
                continue
            }

            $signature = "{0}:{1}:{2}" -f $scan.Target, $scan.X, $scan.Y
            if ($scan.Target -eq "install-now" -and $signature -eq $lastClickSignature) {
                $repeatInstallClicks++
            } else {
                $repeatInstallClicks = 1
            }
            $lastClickSignature = $signature

            if ($scan.Target -eq "install-now" -and $repeatInstallClicks -ge 2) {
                Write-Log "Install button did not advance; will try the agreement checkbox before clicking it again."
                $preferAgreement = $true
            } else {
                $preferAgreement = $false
            }

            Start-Sleep -Seconds 2
            continue
        }

        if ($scan.State -eq "no-window") {
            $idleRounds++
            if ($idleRounds % 10 -eq 0) {
                Write-Log "No WeGame installer window found yet."
            }
            Start-Sleep -Milliseconds 1200
            continue
        }

        $seenWindow = $true
        $idleRounds++
        if ($scan.State -eq "no-agreement") {
            $preferAgreement = $false
            $repeatInstallClicks = 0
            Start-Sleep -Milliseconds 1200
            continue
        }

        if ($idleRounds % 10 -eq 0) {
            Write-Log "No whitelisted install button recognized; waiting."
        }

        if ($seenWindow -and $lastClick -and (((Get-Date) - $lastClick).TotalSeconds -gt 120) -and $idleRounds -gt 40) {
            Write-Log "No new button for a while; assuming the install flow is done."
            return 0
        }

        Start-Sleep -Milliseconds 1200
    }

    Write-Log "Timed out."
    return 1
}

try {
    $code = Main
    Pause-Exit $code
} catch {
    Write-Log "ERROR: $($_.Exception.Message)"
    Pause-Exit 1
}
