Add-Type -AssemblyName System.Drawing
$root = $PSScriptRoot
function Write-Artwork($name, $width, $height, $sidebar) {
  $bitmap = New-Object Drawing.Bitmap($width, $height)
  $g = [Drawing.Graphics]::FromImage($bitmap)
  $g.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.TextRenderingHint = [Drawing.Text.TextRenderingHint]::AntiAliasGridFit
  $bg = [Drawing.ColorTranslator]::FromHtml('#141719')
  $mint = New-Object Drawing.SolidBrush([Drawing.ColorTranslator]::FromHtml('#99d5b4'))
  $text = New-Object Drawing.SolidBrush([Drawing.ColorTranslator]::FromHtml('#E7EDEB'))
  $muted = New-Object Drawing.SolidBrush([Drawing.ColorTranslator]::FromHtml('#A0ADAA'))
  $g.Clear($bg)
  if ($sidebar) {
    $g.FillEllipse($mint, 22, 30, 44, 44)
    $pen = New-Object Drawing.Pen($bg, 4)
    $g.DrawLines($pen, [Drawing.Point[]]@((New-Object Drawing.Point(33,52)),(New-Object Drawing.Point(41,60)),(New-Object Drawing.Point(56,43))))
    $font = New-Object Drawing.Font('Segoe UI', 23, [Drawing.FontStyle]::Bold, [Drawing.GraphicsUnit]::Pixel)
    $small = New-Object Drawing.Font('Segoe UI', 12, [Drawing.FontStyle]::Regular, [Drawing.GraphicsUnit]::Pixel)
    $g.DrawString("TukTuk`nTrack", $font, $text, 20, 93)
    $g.DrawString("Zaplanuj.`nZr$([char]0xF3)b.`nZapisz.", $small, $muted, 22, 175)
    $g.FillRectangle($mint, 22, 279, 120, 3)
    $pen.Dispose(); $font.Dispose(); $small.Dispose()
  } else {
    $font = New-Object Drawing.Font('Segoe UI', 18, [Drawing.FontStyle]::Bold, [Drawing.GraphicsUnit]::Pixel)
    $g.DrawString('TukTuk Track', $font, $text, 17, 15)
    $g.FillRectangle($mint, 17, 44, 116, 2)
    $font.Dispose()
  }
  $bitmap.Save((Join-Path $root $name), [Drawing.Imaging.ImageFormat]::Bmp)
  $g.Dispose(); $bitmap.Dispose(); $mint.Dispose(); $text.Dispose(); $muted.Dispose()
}
Write-Artwork 'sidebar.bmp' 164 314 $true
Write-Artwork 'header.bmp' 150 57 $false

# Ikona traya w czasie pomiaru: ikona aplikacji z kropką w prawym dolnym rogu.
# Plik ICO z wpisami PNG; Tauri (include_image!) bierze największy wpis.
function Write-ActiveIcon($source, $target) {
  $sizes = 16, 24, 32, 48, 64, 256
  $base = [Drawing.Image]::FromFile($source)
  $images = @()
  foreach ($size in $sizes) {
    $bitmap = New-Object Drawing.Bitmap($size, $size, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [Drawing.Graphics]::FromImage($bitmap)
    $g.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.Clear([Drawing.Color]::Transparent)
    $g.DrawImage($base, 0, 0, $size, $size)
    $d = [Math]::Max(7, [Math]::Round($size * 0.5))
    $ring = [Math]::Max(1, [Math]::Round($size * 0.06))
    $x = $size - $d; $y = $size - $d
    $cream = New-Object Drawing.SolidBrush([Drawing.ColorTranslator]::FromHtml('#F4F1E6'))
    $red = New-Object Drawing.SolidBrush([Drawing.ColorTranslator]::FromHtml('#E5484D'))
    $g.FillEllipse($cream, $x, $y, $d, $d)
    $g.FillEllipse($red, $x + $ring, $y + $ring, $d - 2 * $ring, $d - 2 * $ring)
    $stream = New-Object IO.MemoryStream
    $bitmap.Save($stream, [Drawing.Imaging.ImageFormat]::Png)
    $images += , $stream.ToArray()
    $stream.Dispose(); $cream.Dispose(); $red.Dispose(); $g.Dispose(); $bitmap.Dispose()
  }
  $base.Dispose()
  $out = New-Object IO.MemoryStream
  $w = New-Object IO.BinaryWriter($out)
  $w.Write([UInt16]0); $w.Write([UInt16]1); $w.Write([UInt16]$sizes.Count)
  $offset = 6 + 16 * $sizes.Count
  for ($i = 0; $i -lt $sizes.Count; $i++) {
    $edge = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $w.Write([Byte]$edge); $w.Write([Byte]$edge); $w.Write([Byte]0); $w.Write([Byte]0)
    $w.Write([UInt16]1); $w.Write([UInt16]32)
    $w.Write([UInt32]$images[$i].Length); $w.Write([UInt32]$offset)
    $offset += $images[$i].Length
  }
  foreach ($bytes in $images) { $w.Write([byte[]]$bytes) }
  $w.Flush()
  [IO.File]::WriteAllBytes($target, $out.ToArray())
  $w.Dispose(); $out.Dispose()
}
$icons = Join-Path (Split-Path $root -Parent) 'icons'
Write-ActiveIcon (Join-Path $icons 'icon.png') (Join-Path $icons 'icon-active.ico')
