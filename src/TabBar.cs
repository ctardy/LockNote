using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace LockNote
{
    class TabEventArgs : EventArgs
    {
        public int TabIndex { get; private set; }
        public TabEventArgs(int index) { TabIndex = index; }
    }

    class TabBar : BufferedPanel
    {
        private List<string> tabNames = new List<string>();
        private List<bool> tabModified = new List<bool>();
        private int activeIndex;
        private int hoverIndex = -1;
        private bool hoverClose;
        private bool hoverAdd;

        private const int TabHeight = 30;
        private const int MinTabWidth = 80;
        private const int MaxTabWidth = 200;
        private const int CloseButtonSize = 16;
        private const int AddButtonWidth = 28;
        private const int TabPadding = 12;

        public event EventHandler<TabEventArgs> ActiveTabChanged;
        public event EventHandler<TabEventArgs> TabCloseRequested;
        public event EventHandler<TabEventArgs> TabRenameRequested;
        public event EventHandler NewTabRequested;

        public int ActiveIndex { get { return activeIndex; } }
        public int TabCount { get { return tabNames.Count; } }

        public TabBar()
        {
            Dock = DockStyle.Top;
            Height = TabHeight;
            SetStyle(ControlStyles.ResizeRedraw, true);
        }

        public void SetTabs(List<string> names, List<bool> modified, int active)
        {
            tabNames = names ?? new List<string>();
            tabModified = modified ?? new List<bool>();
            activeIndex = active;
            Invalidate();
        }

        private int MeasureTabWidth(Graphics g, string name, bool modified)
        {
            string text = modified ? "\u25CF " + name : name;
            int textWidth = (int)Math.Ceiling(g.MeasureString(text, Theme.UIFont).Width);
            int width = textWidth + TabPadding * 2 + CloseButtonSize + 4;
            if (width < MinTabWidth) width = MinTabWidth;
            if (width > MaxTabWidth) width = MaxTabWidth;
            return width;
        }

        private List<Rectangle> GetTabRects(Graphics g)
        {
            var rects = new List<Rectangle>();
            int x = 0;
            for (int i = 0; i < tabNames.Count; i++)
            {
                int w = MeasureTabWidth(g, tabNames[i], tabModified.Count > i && tabModified[i]);
                rects.Add(new Rectangle(x, 0, w, TabHeight));
                x += w;
            }
            return rects;
        }

        private Rectangle GetAddButtonRect(Graphics g)
        {
            var rects = GetTabRects(g);
            int x = 0;
            if (rects.Count > 0)
            {
                var last = rects[rects.Count - 1];
                x = last.Right;
            }
            return new Rectangle(x, 0, AddButtonWidth, TabHeight);
        }

        private Rectangle GetCloseRect(Rectangle tabRect)
        {
            int cx = tabRect.Right - CloseButtonSize - 4;
            int cy = (TabHeight - CloseButtonSize) / 2;
            return new Rectangle(cx, cy, CloseButtonSize, CloseButtonSize);
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            base.OnPaint(e);
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.ClearTypeGridFit;

            // Fill background
            using (var bgBrush = new SolidBrush(Theme.SurfaceLight))
            {
                g.FillRectangle(bgBrush, ClientRectangle);
            }

            var rects = GetTabRects(g);

            for (int i = 0; i < rects.Count; i++)
            {
                Rectangle r = rects[i];
                bool isActive = (i == activeIndex);
                bool isHover = (i == hoverIndex);
                bool modified = tabModified.Count > i && tabModified[i];

                // Tab background
                Color tabBg = isActive ? Theme.Background : Theme.Surface;
                if (isHover && !isActive)
                {
                    tabBg = Theme.SurfaceLight;
                }
                using (var brush = new SolidBrush(tabBg))
                {
                    g.FillRectangle(brush, r);
                }

                // Active tab bottom accent
                if (isActive)
                {
                    using (var pen = new Pen(Theme.Accent, 2f))
                    {
                        g.DrawLine(pen, r.Left, r.Bottom - 1, r.Right, r.Bottom - 1);
                    }
                }

                // Tab text
                string displayName = modified ? "\u25CF " + tabNames[i] : tabNames[i];
                Color textColor = isActive ? Theme.TextPrimary : Theme.TextSecondary;

                // If modified, draw the dot in accent color separately
                int textX = r.X + TabPadding;
                int textY = (TabHeight - Theme.UIFont.Height) / 2;
                int maxTextWidth = r.Width - TabPadding * 2 - CloseButtonSize - 4;

                if (modified)
                {
                    string dot = "\u25CF ";
                    using (var accentBrush = new SolidBrush(Theme.Accent))
                    {
                        g.DrawString(dot, Theme.UIFont, accentBrush, textX, textY);
                    }
                    int dotWidth = (int)Math.Ceiling(g.MeasureString(dot, Theme.UIFont).Width) - 3;
                    using (var textBrush = new SolidBrush(textColor))
                    {
                        var textRect = new RectangleF(textX + dotWidth, textY, maxTextWidth - dotWidth, TabHeight);
                        var sf = new StringFormat();
                        sf.Trimming = StringTrimming.EllipsisCharacter;
                        sf.FormatFlags = StringFormatFlags.NoWrap;
                        g.DrawString(tabNames[i], Theme.UIFont, textBrush, textRect, sf);
                    }
                }
                else
                {
                    using (var textBrush = new SolidBrush(textColor))
                    {
                        var textRect = new RectangleF(textX, textY, maxTextWidth, TabHeight);
                        var sf = new StringFormat();
                        sf.Trimming = StringTrimming.EllipsisCharacter;
                        sf.FormatFlags = StringFormatFlags.NoWrap;
                        g.DrawString(tabNames[i], Theme.UIFont, textBrush, textRect, sf);
                    }
                }

                // Close button (only on hover)
                if (isHover || isActive)
                {
                    Rectangle closeRect = GetCloseRect(r);
                    bool closeHover = (isHover && hoverClose);
                    Color closeColor = closeHover ? Theme.TextPrimary : Theme.TextMuted;
                    using (var pen = new Pen(closeColor, 1.5f))
                    {
                        int m = 4;
                        g.DrawLine(pen, closeRect.X + m, closeRect.Y + m,
                            closeRect.Right - m, closeRect.Bottom - m);
                        g.DrawLine(pen, closeRect.Right - m, closeRect.Y + m,
                            closeRect.X + m, closeRect.Bottom - m);
                    }
                }
            }

            // Draw "+" add button
            Rectangle addRect = GetAddButtonRect(g);
            Color addColor = hoverAdd ? Theme.TextPrimary : Theme.TextMuted;
            using (var brush = new SolidBrush(addColor))
            {
                var sf = new StringFormat();
                sf.Alignment = StringAlignment.Center;
                sf.LineAlignment = StringAlignment.Center;
                g.DrawString("+", Theme.UIFontBold, brush, addRect, sf);
            }

            // Bottom border
            using (var pen = new Pen(Theme.Border))
            {
                g.DrawLine(pen, 0, Height - 1, Width, Height - 1);
            }
        }

        private int HitTestTab(Point pt, out bool closeButton)
        {
            closeButton = false;
            using (var g = CreateGraphics())
            {
                var rects = GetTabRects(g);
                for (int i = 0; i < rects.Count; i++)
                {
                    if (rects[i].Contains(pt))
                    {
                        Rectangle cr = GetCloseRect(rects[i]);
                        closeButton = cr.Contains(pt);
                        return i;
                    }
                }
            }
            return -1;
        }

        private bool HitTestAdd(Point pt)
        {
            using (var g = CreateGraphics())
            {
                Rectangle addRect = GetAddButtonRect(g);
                return addRect.Contains(pt);
            }
        }

        protected override void OnMouseMove(MouseEventArgs e)
        {
            base.OnMouseMove(e);
            bool close;
            int idx = HitTestTab(e.Location, out close);
            bool add = HitTestAdd(e.Location);

            if (idx != hoverIndex || close != hoverClose || add != hoverAdd)
            {
                hoverIndex = idx;
                hoverClose = close;
                hoverAdd = add;
                Invalidate();
            }
        }

        protected override void OnMouseLeave(EventArgs e)
        {
            base.OnMouseLeave(e);
            hoverIndex = -1;
            hoverClose = false;
            hoverAdd = false;
            Invalidate();
        }

        protected override void OnMouseClick(MouseEventArgs e)
        {
            base.OnMouseClick(e);

            if (e.Button == MouseButtons.Left)
            {
                bool close;
                int idx = HitTestTab(e.Location, out close);

                if (idx >= 0 && close)
                {
                    if (TabCloseRequested != null)
                        TabCloseRequested(this, new TabEventArgs(idx));
                    return;
                }

                if (idx >= 0 && idx != activeIndex)
                {
                    if (ActiveTabChanged != null)
                        ActiveTabChanged(this, new TabEventArgs(idx));
                    return;
                }

                if (HitTestAdd(e.Location))
                {
                    if (NewTabRequested != null)
                        NewTabRequested(this, EventArgs.Empty);
                    return;
                }
            }
            else if (e.Button == MouseButtons.Right)
            {
                bool close;
                int idx = HitTestTab(e.Location, out close);
                if (idx >= 0)
                {
                    ShowTabContextMenu(idx, e.Location);
                }
            }
        }

        protected override void OnMouseDoubleClick(MouseEventArgs e)
        {
            base.OnMouseDoubleClick(e);
            if (e.Button == MouseButtons.Left)
            {
                bool close;
                int idx = HitTestTab(e.Location, out close);
                if (idx >= 0 && !close)
                {
                    if (TabRenameRequested != null)
                        TabRenameRequested(this, new TabEventArgs(idx));
                }
            }
        }

        private void ShowTabContextMenu(int index, Point location)
        {
            var ctx = new ContextMenuStrip();
            var renameItem = new ToolStripMenuItem("Rename");
            var closeItem = new ToolStripMenuItem("Close");

            int capturedIndex = index;
            renameItem.Click += delegate
            {
                if (TabRenameRequested != null)
                    TabRenameRequested(this, new TabEventArgs(capturedIndex));
            };
            closeItem.Click += delegate
            {
                if (TabCloseRequested != null)
                    TabCloseRequested(this, new TabEventArgs(capturedIndex));
            };

            if (tabNames.Count <= 1)
                closeItem.Enabled = false;

            ctx.Items.Add(renameItem);
            ctx.Items.Add(closeItem);
            Theme.ApplyToContextMenu(ctx);
            ctx.Show(this, location);
        }
    }
}
