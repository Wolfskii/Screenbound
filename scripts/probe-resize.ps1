param([Parameter(Mandatory)][int]$ProcessId, [int]$Width = 1440, [int]$Height = 0, [int]$WaitMs = 1500, [switch]$NoSendChanging)
# Dev probe: resize the main window of one process and report how its bounds evolve.
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class SbProbe {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern IntPtr GetWindowLongPtr(IntPtr h, int i);
}
"@
$h = (Get-Process -Id $ProcessId).MainWindowHandle
if ($h -eq [IntPtr]::Zero) { throw "no main window for pid $ProcessId" }
function Show($label) {
  $r = New-Object SbProbe+RECT; [void][SbProbe]::GetWindowRect($h, [ref]$r)
  $style = [SbProbe]::GetWindowLongPtr($h, -16).ToInt64()
  "{0,-10} ({1},{2}) {3}x{4} style=0x{5:X8}" -f $label, $r.L, $r.T, ($r.R - $r.L), ($r.B - $r.T), $style
}
Show "before"
$r0 = New-Object SbProbe+RECT; [void][SbProbe]::GetWindowRect($h, [ref]$r0)
if ($Height -le 0) { $Height = $r0.B - $r0.T }
# SWP_NOZORDER|SWP_NOACTIVATE|SWP_NOOWNERZORDER|SWP_FRAMECHANGED (+ SWP_NOSENDCHANGING)
$flags = 0x0004 -bor 0x0010 -bor 0x0200 -bor 0x0020
if ($NoSendChanging) { $flags = $flags -bor 0x0400 }
$ok = [SbProbe]::SetWindowPos($h, [IntPtr]::Zero, $r0.L, $r0.T, $Width, $Height, $flags)
Show "set($ok)"
foreach ($t in 50, 200, 500, $WaitMs) { Start-Sleep -Milliseconds $t; Show "+${t}ms" }
