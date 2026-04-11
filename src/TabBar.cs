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

        public TabEventArgs(int tabIndex)
        {
            TabIndex = tabIndex;
        }
    }

    class TabBar : BufferedPanel
    {
        List<string> tabNames = new List<string>();
        List<bool> tabModified = new List<bool>();
        int activeIndex;
        List<Rectangle> tabRects = new List<Rectangle>();
        List<Rectangle> closeRects = new List<Rectangle>();
        Rectangle addRect;
        int hoverIndex = -1;
        int hoverCloseIndex = -1;

        const int TabHeight = 30;
        const int TabPadding = 12;
        const int CloseSize = 14;
        const int AddButtonWidth = 28;

        public int ActiveIndex { get { return activeIndex; } }
        public int TabCount { get { return tabNames.Count; } }

        public event EventHandler<TabEventArgs> ActiveTabChanged;
        public event EventHandler<TabEventArgs> TabCloseRequested;
        public event EventHandler<TabEventArgs> TabRenameRequested;
        public event EventHandler NewTabRequested;

        public TabBar()
        {
            Height = TabHeight;
            Dock = DockStyle.Top;
            BackColor = Theme.Background;
        }

        public void SetTabs(List<string> names, List<bool> modified, int active)
        {
            tabNames = new List<string>(names);
            tabModified = new List<bool>(modified);
            activeIndex = active;
            Invalidate();
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            base.OnPaint(e);
            var g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.ClearTypeGridFit;

            using (var bgBrush = new SolidBrush(Theme.Background))
            {
                g.FillRectangle(bgBrush, ClientRectangle);
            }

            tabRects.Clear();
            closeRects.Clear();

            int x = 4;
            for (int i = 0; i < tabNames.Count; i++)
            {
                string displayName = tabNames[i];
                if (tabModified[i]) displayName = displayName + " *";

                Size textSize = TextRenderer.MeasureText(displayName, Theme.UIFont);
                int tabWidth = textSize.Width + TabPadding * 2 + CloseSize + 8;

                var rect = new Rectangle(x, 0, tabWidth, TabHeight);
                tabRects.Add(rect);

                bool isActive = (i == activeIndex);
                bool isHover = (i == hoverIndex);

                Color tabBg;
                if (isActive) tabBg = Theme.EditorBackground;
                else if (isHover) tabBg = Theme.SurfaceLight;
                else tabBg = Theme.Surface;

                using (var brush = new SolidBrush(tabBg))
                {
                    g.FillRectangle(brush, rect);
                }

                if (isActive)
                {
                    using (var pen = new Pen(Theme.Accent, 2))
                    {
                        g.DrawLine(pen, rect.Left, rect.Bottom - 1, rect.Right, rect.Bottom - 1);
                    }
                }

                Color textColor = isActive ? Theme.TextPrimary : Theme.TextSecondary;
                TextRenderer.DrawText(g, displayName, Theme.UIFont, new Point(x + TabPadding, (TabHeight - textSize.Height) / 2), textColor);

                // Close button
                int closeX = x + tabWidth - CloseSize - 6;
                int closeY = (TabHeight - CloseSize) / 2;
                var closeRect = new Rectangle(closeX, closeY, CloseSize, CloseSize);
                closeRects.Add(closeRect);

                if (tabNames.Count > 1)
                {
                    Color closeColor = (i == hoverCloseIndex) ? Theme.TextPrimary : Theme.TextMuted;
                    using (var pen = new Pen(closeColor, 1.5f))
                    {
                        int m = 3;
                        g.DrawLine(pen, closeRect.Left + m, closeRect.Top + m, closeRect.Right - m, closeRect.Bottom - m);
                        g.DrawLine(pen, closeRect.Right - m, closeRect.Top + m, closeRect.Left + m, closeRect.Bottom - m);
                    }
                }

                x += tabWidth + 2;
            }

            // Add button
            addRect = new Rectangle(x + 4, (TabHeight - 20) / 2, AddButtonWidth, 20);
            using (var brush = new SolidBrush(Theme.Surface))
            {
                g.FillRectangle(brush, addRect);
            }
            TextRenderer.DrawText(g, "+", Theme.UIFont, addRect, Theme.TextSecondary, TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);

            // Bottom border
            using (var pen = new Pen(Theme.Border))
            {
                g.DrawLine(pen, 0, TabHeight - 1, Width, TabHeight - 1);
            }
        }

        protected override void OnMouseMove(MouseEventArgs e)
        {
            base.OnMouseMove(e);
            int oldHover = hoverIndex;
            int oldCloseHover = hoverCloseIndex;
            hoverIndex = -1;
            hoverCloseIndex = -1;

            for (int i = 0; i < tabRects.Count; i++)
            {
                if (tabRects[i].Contains(e.Location))
                {
                    hoverIndex = i;
                    if (closeRects[i].Contains(e.Location))
                        hoverCloseIndex = i;
                    break;
                }
            }

            if (hoverIndex != oldHover || hoverCloseIndex != oldCloseHover)
                Invalidate();
        }

        protected override void OnMouseLeave(EventArgs e)
        {
            base.OnMouseLeave(e);
            if (hoverIndex != -1 || hoverCloseIndex != -1)
            {
                hoverIndex = -1;
                hoverCloseIndex = -1;
                Invalidate();
            }
        }

        protected override void OnMouseClick(MouseEventArgs e)
        {
            base.OnMouseClick(e);

            if (e.Button == MouseButtons.Right)
            {
                for (int i = 0; i < tabRects.Count; i++)
                {
                    if (tabRects[i].Contains(e.Location))
                    {
                        ShowContextMenu(i, e.Location);
                        return;
                    }
                }
                return;
            }

            if (e.Button != MouseButtons.Left) return;

            // Check add button
            if (addRect.Contains(e.Location))
            {
                if (NewTabRequested != null) NewTabRequested(this, EventArgs.Empty);
                return;
            }

            for (int i = 0; i < tabRects.Count; i++)
            {
                if (closeRects[i].Contains(e.Location) && tabNames.Count > 1)
                {
                    if (TabCloseRequested != null) TabCloseRequested(this, new TabEventArgs(i));
                    return;
                }
                if (tabRects[i].Contains(e.Location))
                {
                    if (i != activeIndex)
                    {
                        if (ActiveTabChanged != null) ActiveTabChanged(this, new TabEventArgs(i));
                    }
                    return;
                }
            }
        }

        void ShowContextMenu(int index, Point location)
        {
            var menu = new ContextMenuStrip();
            int idx = index;
            menu.Items.Add("Rename", null, delegate { if (TabRenameRequested != null) TabRenameRequested(this, new TabEventArgs(idx)); });
            if (tabNames.Count > 1)
                menu.Items.Add("Close", null, delegate { if (TabCloseRequested != null) TabCloseRequested(this, new TabEventArgs(idx)); });
            menu.Items.Add(new ToolStripSeparator());
            menu.Items.Add("New tab", null, delegate { if (NewTabRequested != null) NewTabRequested(this, EventArgs.Empty); });
            Theme.ApplyToContextMenu(menu);
            menu.Show(this, location);
        }
    }
}
