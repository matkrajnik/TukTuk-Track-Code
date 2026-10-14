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
    $g.DrawString("Todo`nTrack", $font, $text, 20, 93)
    $g.DrawString("Zaplanuj.`nZr$([char]0xF3)b.`nZapisz.", $small, $muted, 22, 175)
    $g.FillRectangle($mint, 22, 279, 120, 3)
    $pen.Dispose(); $font.Dispose(); $small.Dispose()
  } else {
    $font = New-Object Drawing.Font('Segoe UI', 18, [Drawing.FontStyle]::Bold, [Drawing.GraphicsUnit]::Pixel)
    $g.DrawString('Todo Track', $font, $text, 17, 15)
    $g.FillRectangle($mint, 17, 44, 116, 2)
    $font.Dispose()
  }
  $bitmap.Save((Join-Path $root $name), [Drawing.Imaging.ImageFormat]::Bmp)
  $g.Dispose(); $bitmap.Dispose(); $mint.Dispose(); $text.Dispose(); $muted.Dispose()
}
Write-Artwork 'sidebar.bmp' 164 314 $true
Write-Artwork 'header.bmp' 150 57 $false
