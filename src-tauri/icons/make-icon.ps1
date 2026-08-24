Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.Clear([System.Drawing.Color]::Transparent)

# Rounded dark square background.
$bgBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 17, 17, 20))
$borderPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 58, 58, 66), 24)
$radius = 220
$rect = New-Object System.Drawing.Rectangle(64, 64, ($size - 128), ($size - 128))
$path = New-Object System.Drawing.Drawing2D.GraphicsPath
$path.AddArc($rect.X, $rect.Y, $radius, $radius, 180, 90)
$path.AddArc($rect.Right - $radius, $rect.Y, $radius, $radius, 270, 90)
$path.AddArc($rect.Right - $radius, $rect.Bottom - $radius, $radius, $radius, 0, 90)
$path.AddArc($rect.X, $rect.Bottom - $radius, $radius, $radius, 90, 90)
$path.CloseFigure()
$g.FillPath($bgBrush, $path)
$g.DrawPath($borderPen, $path)

# White "play" triangle.
$white = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 235, 235, 238))
$tri = @(
  (New-Object System.Drawing.PointF(380, 300)),
  (New-Object System.Drawing.PointF(380, 724)),
  (New-Object System.Drawing.PointF(720, 512))
)
$g.FillPolygon($white, $tri)

# Green status dot.
$green = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 74, 222, 128))
$g.FillEllipse($green, 700, 700, 130, 130)

$g.Dispose()
$out = Join-Path $PSScriptRoot 'app-icon.png'
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "saved: $out"
