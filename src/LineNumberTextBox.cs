using System;
using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    /// <summary>
    /// A RichTextBox with a line number gutter on the left.
    /// </summary>
    class LineNumberTextBox : UserControl
    {
        Panel gutter;
        RichTextBox rtb;

        const int GutterPadding = 6;

        public event EventHandler ContentChanged;

        public LineNumberTextBox()
        {
            rtb = new RichTextBox
            {
                Dock = DockStyle.Fill,
                BorderStyle = BorderStyle.None,
                WordWrap = false,
                AcceptsTab = true,
                ScrollBars = RichTextBoxScrollBars.Both,
                DetectUrls = false,
                MaxLength = 0,
                ShortcutsEnabled = true
            };

            gutter = new Panel
            {
                Dock = DockStyle.Left,
                Width = 50,
                BackColor = Color.FromArgb(240, 240, 240)
            };

            rtb.TextChanged += (s, e) =>
            {
                gutter.Invalidate();
                if (ContentChanged != null) ContentChanged(this, EventArgs.Empty);
            };
            rtb.VScroll += (s, e) => gutter.Invalidate();
            rtb.Resize += (s, e) => gutter.Invalidate();

            gutter.Paint += OnGutterPaint;

            Controls.Add(rtb);
            Controls.Add(gutter);
        }

        // ── Public properties (no override to avoid recursion) ──

        public string ContentText
        {
            get { return rtb.Text; }
            set { rtb.Text = value; }
        }

        public Font EditorFont
        {
            get { return rtb.Font; }
            set
            {
                rtb.Font = value;
                gutter.Invalidate();
            }
        }

        public Color TextForeColor
        {
            get { return rtb.ForeColor; }
            set { rtb.ForeColor = value; }
        }

        public Color TextBackColor
        {
            get { return rtb.BackColor; }
            set { rtb.BackColor = value; }
        }

        public int ContentLength { get { return rtb.TextLength; } }

        public void SelectAll() { rtb.SelectAll(); }

        public void SelectText(int start, int length)
        {
            rtb.Select(start, length);
            rtb.ScrollToCaret();
        }

        public void ScrollToCaret() { rtb.ScrollToCaret(); }

        public void Clear() { rtb.Clear(); }

        public new bool Focus() { return rtb.Focus(); }

        // ── Gutter painting ──

        void OnGutterPaint(object sender, PaintEventArgs e)
        {
            Graphics g = e.Graphics;
            g.Clear(gutter.BackColor);

            int totalLines = rtb.Lines.Length;
            if (totalLines == 0) totalLines = 1;

            // Adjust gutter width based on digit count
            int digits = totalLines.ToString().Length;
            int newWidth = Math.Max(40, digits * 10 + GutterPadding * 2 + 4);
            if (gutter.Width != newWidth)
            {
                gutter.Width = newWidth;
                return;
            }

            int firstCharIndex = rtb.GetCharIndexFromPosition(new Point(0, 0));
            int firstLine = rtb.GetLineFromCharIndex(firstCharIndex);

            float lineHeight = rtb.Font.GetHeight(g);
            int visibleLines = (int)(rtb.ClientSize.Height / lineHeight) + 2;

            for (int i = 0; i < visibleLines; i++)
            {
                int lineIndex = firstLine + i;
                if (lineIndex >= totalLines) break;

                int charIndex = rtb.GetFirstCharIndexFromLine(lineIndex);
                if (charIndex < 0) break;

                Point pos = rtb.GetPositionFromCharIndex(charIndex);
                if (pos.Y + lineHeight < 0) continue;
                if (pos.Y > rtb.ClientSize.Height) break;

                DrawLineNumber(g, lineIndex + 1, pos.Y);
            }
        }

        void DrawLineNumber(Graphics g, int number, float y)
        {
            string text = number.ToString();
            SizeF size = g.MeasureString(text, rtb.Font);
            float x = gutter.Width - size.Width - GutterPadding;
            using (var brush = new SolidBrush(Color.FromArgb(130, 130, 130)))
            {
                g.DrawString(text, rtb.Font, brush, x, y);
            }
        }
    }
}
