<#
.SYNOPSIS
    Launch BachelorPad+, drive it, and capture what it drew.

.DESCRIPTION
    A manual pass at the window has found a defect no test could, every time
    it has been run -- `project/NEXT_SESSION.md` §3 has the tally. Every
    session that has done it re-derived the Win32 plumbing from scratch, so
    this is that plumbing, once.

    **What this can and cannot do**, both learned the hard way:

    - It *can* launch the application, find its window, bring it forward,
      click at a point, send keys, and capture the window to a PNG.
    - It **cannot get past an `rfd` modal.** Native dialogs swallow synthetic
      input. A row that opens one can be clicked and the dialog can be
      *photographed*, but it cannot be dismissed -- so a run that opens one
      ends with `-Kill`.
    - `SetForegroundWindow` **fails silently**, and a lost foreground is
      indistinguishable from the application dropping keystrokes. One "defect"
      was recorded and later withdrawn for exactly that. Every send here
      checks the foreground handle on both sides and refuses rather than
      typing into whatever else happened to be in front.

.PARAMETER File
    A document to open. Optional.

.PARAMETER Click
    "x,y" in *window* coordinates. May be given more than once; each click is
    followed by a capture, so a menu and the row it opens are both recorded.

.PARAMETER Keys
    Sent through SendKeys after any clicks. See the SendKeys grammar; `%`
    is Alt, `^` is Ctrl.

.PARAMETER Out
    Directory for the captures. Defaults to the system temp directory.

.PARAMETER Settle
    Milliseconds to wait after each action before capturing. The default is
    generous because a capture taken mid-draw is a bug report about nothing.

.PARAMETER Screen
    Capture the whole primary screen rather than the window.

    **Required for anything that opens an `rfd` dialog**, which is most of the
    report rows: a native dialog is its own top-level window, so `PrintWindow`
    on the application's window photographs the window *behind* it and the
    dialog is simply absent. That reads as "the row did nothing", which is the
    one conclusion a capture must never invite by accident.

.PARAMETER Kill
    Terminate the process at the end. Required to get out of a modal.

.EXAMPLE
    ./scripts/Drive-Window.ps1 -Click "300,10" -Out ./shots -Kill
    Open the menu at x=300 and photograph it.
#>
[CmdletBinding()]
param(
    [string]$File,
    [string[]]$Click = @(),
    [string]$Keys,
    [string]$Out = $env:TEMP,
    [int]$Settle = 900,
    [switch]$Screen,
    [switch]$Kill
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $Root 'target/release/bachelorpad.exe'
if (-not (Test-Path $exe)) {
    throw "$exe is missing -- cargo build --release -p bp-ui"
}

Add-Type -AssemblyName System.Windows.Forms, System.Drawing

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Win {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr pid);
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool attach);
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr h);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int max);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, System.Text.StringBuilder s, int max);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr param);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    public delegate bool EnumProc(IntPtr h, IntPtr param);

    public static System.Collections.Generic.List<IntPtr> VisibleWindowsOf(uint wanted) {
        var found = new System.Collections.Generic.List<IntPtr>();
        EnumWindows(delegate(IntPtr h, IntPtr p) {
            uint pid;
            GetWindowThreadProcessId(h, out pid);
            if (pid == wanted && IsWindowVisible(h)) { found.Add(h); }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    public static string ClassOf(IntPtr h) {
        var sb = new System.Text.StringBuilder(256);
        GetClassName(h, sb, sb.Capacity);
        return sb.ToString();
    }

    public static string TitleOf(IntPtr h) {
        int n = GetWindowTextLength(h);
        if (n <= 0) { return ""; }
        var sb = new System.Text.StringBuilder(n + 1);
        GetWindowText(h, sb, sb.Capacity);
        return sb.ToString();
    }
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
}
'@

New-Item -ItemType Directory -Force -Path $Out | Out-Null

$arguments = @()
if ($File) { $arguments += $File }
$process = Start-Process -FilePath $exe -ArgumentList $arguments -PassThru

# Wait for a window rather than sleeping a guessed amount: a cold start after
# a rebuild is much slower than a warm one, and a fixed sleep is either a
# flake or a waste.
$deadline = [datetime]::UtcNow.AddSeconds(30)
while ([datetime]::UtcNow -lt $deadline -and $process.MainWindowHandle -eq 0) {
    Start-Sleep -Milliseconds 200
    $process.Refresh()
}
if ($process.MainWindowHandle -eq 0) {
    $process | Stop-Process -Force
    throw 'the window never appeared'
}
# **`MainWindowHandle` is not reliably the window**, and trusting it cost an
# afternoon: for this application it reports a handle that accepts posted
# clicks but returns nothing from `GetClientRect`, so a capture came back
# 1x1 and a run that had worked perfectly looked broken. Enumerate instead
# and take the visible top-level window winit actually created -- its class
# is `Window Class`, and the 16x16 `Winit Thread Event Target` beside it is
# the trap that makes "the biggest one" the wrong rule to write down.
$handle = [IntPtr]::Zero
foreach ($candidate in [Win]::VisibleWindowsOf($process.Id)) {
    if ([Win]::ClassOf($candidate) -eq 'Window Class' -and [Win]::TitleOf($candidate)) {
        $handle = $candidate
        break
    }
}
if ($handle -eq [IntPtr]::Zero) { $handle = $process.MainWindowHandle }
if ($handle -eq [IntPtr]::Zero) {
    $process | Stop-Process -Force
    throw 'no usable window found'
}
Write-Host ("window: {0} '{1}' (pid {2})" -f $handle, [Win]::TitleOf($handle), $process.Id)

function Set-Front {
    <#
      Windows refuses `SetForegroundWindow` from a process that does not own
      the foreground -- silently, returning false, which is the whole reason
      the assertion below exists.

      The way through is to attach this thread's input queue to the thread
      that *does* own it, which makes the two count as one for the purposes of
      that rule. Tried a few times because the foreground can change under us
      between the read and the call.
    #>
    $ours = [Win]::GetCurrentThreadId()
    for ($attempt = 1; $attempt -le 6; $attempt++) {
        $front = [Win]::GetForegroundWindow()
        if ($front -eq $handle) { return $true }

        $owner = [Win]::GetWindowThreadProcessId($front, [IntPtr]::Zero)
        $attached = $false
        if ($owner -ne 0 -and $owner -ne $ours) {
            $attached = [Win]::AttachThreadInput($ours, $owner, $true)
        }
        [void][Win]::ShowWindow($handle, 9)      # SW_RESTORE
        [void][Win]::BringWindowToTop($handle)
        [void][Win]::SetForegroundWindow($handle)
        if ($attached) { [void][Win]::AttachThreadInput($ours, $owner, $false) }

        Start-Sleep -Milliseconds (150 * $attempt)
        if ([Win]::GetForegroundWindow() -eq $handle) { return $true }
    }
    return $false
}

function Assert-Foreground {
    <#
      The check the withdrawn defect was withdrawn for. A lost foreground and
      an application dropping keystrokes look identical from outside, so this
      refuses rather than sending into whatever is actually in front.
    #>
    param([string]$When)
    if (Set-Front) { return }
    $front = [Win]::GetForegroundWindow()
    throw "the window is not in front $When (foreground is $front, ours is $handle) -- anything sent now would go somewhere else"
}

function Save-Capture {
    param([string]$Name)
    Start-Sleep -Milliseconds $Settle

    # **A dialog is its own top-level window**, so photographing the
    # application's window gets the one *behind* it and the dialog is simply
    # absent -- which reads as "the row did nothing", the one conclusion a
    # capture must never invite by accident. `CopyFromScreen` would be the
    # obvious answer and throws "handle is invalid" in a session with no
    # interactive desktop, so this enumerates the process's own windows
    # instead and photographs whichever is not the main one.
    $target = $handle
    if ($Screen) {
        # `#32770` is the Win32 dialog class, which is what `rfd` puts a
        # message box in. Matching on the class rather than "any window that
        # is not the main one" matters: a winit application also owns a 16x16
        # "Thread Event Target" window and an IME window, and picking one of
        # those photographs a 16-pixel square while reporting success.
        foreach ($candidate in [Win]::VisibleWindowsOf($process.Id)) {
            if ($candidate -ne $handle -and [Win]::ClassOf($candidate) -eq '#32770') {
                $target = $candidate
                Write-Host ("dialog: {0} '{1}'" -f $candidate, [Win]::TitleOf($candidate))
                break
            }
        }
        if ($target -eq $handle) { Write-Host 'no dialog window found' -ForegroundColor Yellow }
    }

    $rect = New-Object Win+RECT
    [void][Win]::GetClientRect($target, [ref]$rect)
    $width = [Math]::Max($rect.R - $rect.L, 1)
    $height = [Math]::Max($rect.B - $rect.T, 1)

    $bitmap = New-Object System.Drawing.Bitmap($width, $height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    # Flag 1 is PW_CLIENTONLY; 2 (PW_RENDERFULLCONTENT) is what makes it work
    # for a window drawn by a GPU or software renderer rather than by GDI.
    [void][Win]::PrintWindow($target, $dc, 3)
    $graphics.ReleaseHdc($dc)
    $graphics.Dispose()

    $path = Join-Path $Out "$Name.png"
    $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
    Write-Host "captured: $path  (${width}x${height})"
    return $path
}

# **Capturing does not need the foreground and clicking does not either.**
# `PrintWindow` reads the window's own backing store, and a click is posted
# straight onto its message queue. Only `SendKeys` needs focus, because it
# goes to whatever has it -- so that is the only thing gated below.
Save-Capture '00-opened' | Out-Null

try {
$step = 1
foreach ($point in $Click) {
    $parts = $point -split ','
    if ($parts.Count -ne 2) { throw "click '$point' is not 'x,y'" }

    $clientX = [int]$parts[0]
    $clientY = [int]$parts[1]

    # Posted to the window rather than synthesised at the cursor. Moving the
    # real pointer and firing `mouse_event` needs this window to be in front,
    # which is exactly the thing Windows refuses at random and which cost a
    # withdrawn defect report; a posted message goes on its queue whatever is
    # on top. It also leaves the user's actual pointer alone.
    $lparam = [IntPtr](($clientY -shl 16) -bor ($clientX -band 0xFFFF))
    [void][Win]::PostMessage($handle, 0x0200, [IntPtr]::Zero, $lparam)  # MOUSEMOVE
    Start-Sleep -Milliseconds 80
    [void][Win]::PostMessage($handle, 0x0201, [IntPtr]1, $lparam)       # LBUTTONDOWN
    Start-Sleep -Milliseconds 80
    [void][Win]::PostMessage($handle, 0x0202, [IntPtr]::Zero, $lparam)  # LBUTTONUP

    Save-Capture ('{0:d2}-click-{1}' -f $step, ($point -replace ',', 'x')) | Out-Null
    $step++
}

if ($Keys) {
    Assert-Foreground 'before sending keys'
    [System.Windows.Forms.SendKeys]::SendWait($Keys)
    Save-Capture ('{0:d2}-keys' -f $step) | Out-Null
}

if ($Kill) {
    $process | Stop-Process -Force
    Write-Host 'process terminated'
}
else {
    Write-Host "left running: pid $($process.Id)"
}
}
catch {
    # **A leaked process holds `bachelorpad.exe` open**, and the next
    # `cargo build --release` then fails with "Access is denied" -- which
    # reads as a permissions problem and is not one. It happened twice
    # before this block existed.
    $process | Stop-Process -Force -ErrorAction SilentlyContinue
    Write-Host 'process terminated after a failure' -ForegroundColor Yellow
    throw
}
