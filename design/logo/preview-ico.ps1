# Wyciąga klatki 16 i 32 px z plików icon.ico wygenerowanych przez `npm run tauri icon`
# i składa je powiększone 8x bez wygładzania (jak lupa) na jasnym i ciemnym pasku zadań.
# Użycie: powershell -File preview-ico.ps1 -Dirs "<katalog1>,<katalog2>" -Out sheet.png
param([string[]]$Dirs, [string]$Out = "ico-sheet.png")
Add-Type -AssemblyName System.Drawing
$Dirs = @($Dirs | ForEach-Object { $_ -split "," })
$sizes = 16, 32
$scale = 8
$cell = 32 * $scale + 24

# Klatki ICO zapisane jako PNG (tak robi Tauri) GDI+ czyta błędnie, więc katalog ICO
# parsujemy sami i każdą klatkę ładujemy jako osobny obraz.
function Read-IcoFrames([string]$path) {
  $bytes = [IO.File]::ReadAllBytes($path)
  $count = [BitConverter]::ToUInt16($bytes, 4)
  $frames = @{}
  for ($i = 0; $i -lt $count; $i++) {
    $e = 6 + 16 * $i
    $w = $bytes[$e]; if ($w -eq 0) { $w = 256 }
    $len = [BitConverter]::ToUInt32($bytes, $e + 8)
    $off = [BitConverter]::ToUInt32($bytes, $e + 12)
    $ms = New-Object IO.MemoryStream($bytes, $off, $len)
    $frames[[int]$w] = [Drawing.Image]::FromStream($ms)
  }
  return $frames
}

$w = $cell * 2 * $sizes.Count
$h = $cell * $Dirs.Count
$sheet = New-Object Drawing.Bitmap($w, $h)
$g = [Drawing.Graphics]::FromImage($sheet)
$g.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
$g.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::Half
$light = New-Object Drawing.SolidBrush([Drawing.ColorTranslator]::FromHtml('#eef0f4'))
$dark = New-Object Drawing.SolidBrush([Drawing.ColorTranslator]::FromHtml('#202228'))
$row = 0
foreach ($dir in $Dirs) {
  $frames = Read-IcoFrames (Join-Path $dir 'icon.ico')
  $col = 0
  foreach ($bg in @($light, $dark)) {
    foreach ($s in $sizes) {
      $x = $col * $cell; $y = $row * $cell
      $g.FillRectangle($bg, $x, $y, $cell, $cell)
      $img = $frames[[int]$s]
      if ($img) { $g.DrawImage($img, $x + 12, $y + 12, $s * $scale, $s * $scale) }
      $col++
    }
  }
  $row++
}
$sheet.Save($Out, [Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $sheet.Dispose()
