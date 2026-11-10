Add-Type -AssemblyName System.Drawing

$csharpSource = @'
using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;

public static class IconBuilder {
    public static Bitmap CreateBitmap(int size) {
        Bitmap bmp = new Bitmap(size, size, PixelFormat.Format32bppArgb);
        using (Graphics g = Graphics.FromImage(bmp)) {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.InterpolationMode = InterpolationMode.HighQualityBicubic;
            g.PixelOffsetMode = PixelOffsetMode.HighQuality;

            float s = (float)size;

            // Deep background circle
            RectangleF bgRect = new RectangleF(0.05f * s, 0.05f * s, 0.90f * s, 0.90f * s);
            using (LinearGradientBrush bgBrush = new LinearGradientBrush(
                bgRect,
                Color.FromArgb(255, 30, 41, 59),
                Color.FromArgb(255, 15, 23, 42),
                45.0f)) {
                g.FillEllipse(bgBrush, bgRect);
            }

            // Outer cyan border ring
            using (Pen ringPen = new Pen(Color.FromArgb(220, 56, 189, 248), Math.Max(1.0f, 0.04f * s))) {
                g.DrawEllipse(ringPen, bgRect);
            }

            // Search / launcher lens circle
            RectangleF searchRect = new RectangleF(0.24f * s, 0.22f * s, 0.38f * s, 0.38f * s);
            using (Pen searchPen = new Pen(Color.FromArgb(255, 56, 189, 248), Math.Max(1.5f, 0.075f * s))) {
                g.DrawEllipse(searchPen, searchRect);
            }

            // Search handle
            using (Pen handlePen = new Pen(Color.FromArgb(255, 56, 189, 248), Math.Max(1.5f, 0.075f * s))) {
                handlePen.StartCap = LineCap.Round;
                handlePen.EndCap = LineCap.Round;
                g.DrawLine(handlePen, 0.53f * s, 0.51f * s, 0.72f * s, 0.70f * s);
            }

            // Stylized "J"
            using (Font font = new Font(FontFamily.GenericSansSerif, 0.25f * s, FontStyle.Bold))
            using (SolidBrush whiteBrush = new SolidBrush(Color.White))
            using (StringFormat sf = new StringFormat {
                Alignment = StringAlignment.Center,
                LineAlignment = StringAlignment.Center
            }) {
                RectangleF textRect = new RectangleF(0.20f * s, 0.20f * s, 0.46f * s, 0.42f * s);
                g.DrawString("J", font, whiteBrush, textRect, sf);
            }
        }
        return bmp;
    }

    public static void Generate(string assetsDir) {
        Directory.CreateDirectory(assetsDir);
        using (Bitmap bmp256 = CreateBitmap(256)) {
            bmp256.Save(Path.Combine(assetsDir, "icon.png"), ImageFormat.Png);
        }

        int[] sizes = new int[] { 256, 128, 64, 48, 32, 16 };
        byte[][] pngBytes = new byte[sizes.Length][];

        for (int i = 0; i < sizes.Length; i++) {
            using (Bitmap b = CreateBitmap(sizes[i]))
            using (MemoryStream ms = new MemoryStream()) {
                b.Save(ms, ImageFormat.Png);
                pngBytes[i] = ms.ToArray();
            }
        }

        string icoPath = Path.Combine(assetsDir, "icon.ico");
        using (FileStream fs = File.Create(icoPath))
        using (BinaryWriter bw = new BinaryWriter(fs)) {
            // ICONDIR
            bw.Write((ushort)0);
            bw.Write((ushort)1);
            bw.Write((ushort)sizes.Length);

            int offset = 6 + (sizes.Length * 16);

            for (int i = 0; i < sizes.Length; i++) {
                int sz = sizes[i];
                byte w = sz >= 256 ? (byte)0 : (byte)sz;
                byte h = sz >= 256 ? (byte)0 : (byte)sz;
                bw.Write(w);
                bw.Write(h);
                bw.Write((byte)0);
                bw.Write((byte)0);
                bw.Write((ushort)1);
                bw.Write((ushort)32);
                bw.Write((uint)pngBytes[i].Length);
                bw.Write((uint)offset);
                offset += pngBytes[i].Length;
            }

            for (int i = 0; i < sizes.Length; i++) {
                bw.Write(pngBytes[i]);
            }
        }
    }
}
'@

Add-Type -TypeDefinition $csharpSource -ReferencedAssemblies System.Drawing
[IconBuilder]::Generate("z:\JLauncher\assets")
Write-Host "Created assets\icon.png and assets\icon.ico successfully!"
