using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace LockNote
{
    /// <summary>
    /// Centralized theme system for all LockNote UI.
    /// Supports Dark and Light modes.
    /// </summary>
    static class Theme
    {
        static bool isDark = true;

        // ── Core palette (mutable — set by SetMode) ──
        public static Color Background;
        public static Color Surface;
        public static Color SurfaceLight;
        public static Color Border;
        public static Color TextPrimary;
        public static Color TextSecondary;
        public static Color TextMuted;
        public static Color Accent;
        public static Color AccentHover;
        public static Color EditorBackground;
        public static Color EditorText;
        public static Color GutterBackground;
        public static Color GutterText;
        public static Color StatusBackground;
        public static Color StatusText;
        public static Color MenuBackground;
        public static Color MenuText;
        public static Color MenuHover;
        public static Color InputBackground;
        public static Color InputBorder;
        public static Color ButtonBackground;
        public static Color ButtonText;
        public static Color ButtonSecondary;
        public static Color ErrorText;

        static Theme()
        {
            SetMode(AppTheme.Dark);
        }

        public static void SetMode(AppTheme mode)
        {
            isDark = (mode == AppTheme.Dark);
            if (isDark)
            {
                Background       = Color.FromArgb(30, 30, 30);
                Surface          = Color.FromArgb(42, 42, 42);
                SurfaceLight     = Color.FromArgb(55, 55, 55);
                Border           = Color.FromArgb(65, 65, 65);
                TextPrimary      = Color.FromArgb(220, 220, 220);
                TextSecondary    = Color.FromArgb(150, 150, 150);
                TextMuted        = Color.FromArgb(100, 100, 100);
                Accent           = Color.FromArgb(86, 156, 214);
                AccentHover      = Color.FromArgb(106, 176, 234);
                EditorBackground = Color.FromArgb(30, 30, 30);
                EditorText       = Color.FromArgb(212, 212, 212);
                GutterBackground = Color.FromArgb(37, 37, 37);
                GutterText       = Color.FromArgb(90, 90, 90);
                StatusBackground = Color.FromArgb(0, 122, 204);
                StatusText       = Color.White;
                MenuBackground   = Color.FromArgb(37, 37, 38);
                MenuText         = Color.FromArgb(220, 220, 220);
                MenuHover        = Color.FromArgb(62, 62, 66);
                InputBackground  = Color.FromArgb(60, 60, 60);
                InputBorder      = Color.FromArgb(80, 80, 80);
                ButtonBackground = Color.FromArgb(0, 122, 204);
                ButtonText       = Color.White;
                ButtonSecondary  = Color.FromArgb(60, 60, 60);
                ErrorText        = Color.FromArgb(244, 71, 71);
            }
            else
            {
                Background       = Color.FromArgb(252, 252, 252);
                Surface          = Color.FromArgb(243, 243, 243);
                SurfaceLight     = Color.FromArgb(230, 230, 230);
                Border           = Color.FromArgb(210, 210, 210);
                TextPrimary      = Color.FromArgb(30, 30, 30);
                TextSecondary    = Color.FromArgb(100, 100, 100);
                TextMuted        = Color.FromArgb(160, 160, 160);
                Accent           = Color.FromArgb(0, 120, 212);
                AccentHover      = Color.FromArgb(16, 110, 190);
                EditorBackground = Color.White;
                EditorText       = Color.FromArgb(30, 30, 30);
                GutterBackground = Color.FromArgb(245, 245, 245);
                GutterText       = Color.FromArgb(170, 170, 170);
                StatusBackground = Color.FromArgb(0, 122, 204);
                StatusText       = Color.White;
                MenuBackground   = Color.FromArgb(243, 243, 243);
                MenuText         = Color.FromArgb(30, 30, 30);
                MenuHover        = Color.FromArgb(220, 220, 220);
                InputBackground  = Color.White;
                InputBorder      = Color.FromArgb(200, 200, 200);
                ButtonBackground = Color.FromArgb(0, 122, 204);
                ButtonText       = Color.White;
                ButtonSecondary  = Color.FromArgb(225, 225, 225);
                ErrorText        = Color.FromArgb(200, 40, 40);
            }
        }

        public static bool IsDark { get { return isDark; } }

        // ── Fonts ──
        public static readonly Font UIFont       = new Font("Segoe UI", 9f);
        public static readonly Font UIFontBold   = new Font("Segoe UI", 9f, FontStyle.Bold);
        public static readonly Font EditorFont   = new Font("Consolas", 11f);
        public static readonly Font StatusFont   = new Font("Segoe UI", 8.5f);

        /// <summary>Apply dark theme to a standard dialog form.</summary>
        public static void ApplyToDialog(Form form)
        {
            form.BackColor = Surface;
            form.ForeColor = TextPrimary;
            form.Font = UIFont;
            ApplyToControls(form.Controls);
        }

        /// <summary>Apply dark theme recursively to all controls.</summary>
        public static void ApplyToControls(Control.ControlCollection controls)
        {
            foreach (Control c in controls)
            {
                if (c is Button)
                {
                    StyleButton((Button)c);
                }
                else if (c is TextBox)
                {
                    StyleTextBox((TextBox)c);
                }
                else if (c is Label)
                {
                    StyleLabel((Label)c);
                }
                else if (c is CheckBox)
                {
                    c.ForeColor = TextPrimary;
                    c.BackColor = Surface;
                }
                else if (c is ComboBox)
                {
                    StyleComboBox((ComboBox)c);
                }
                else if (c is Panel)
                {
                    // Don't override panel colors used for specific purposes (strength bar)
                }

                if (c.Controls.Count > 0)
                    ApplyToControls(c.Controls);
            }
        }

        static void StyleButton(Button btn)
        {
            btn.FlatStyle = FlatStyle.Flat;
            btn.FlatAppearance.BorderSize = 0;
            btn.Font = UIFont;
            btn.Cursor = Cursors.Hand;

            if (btn.DialogResult == DialogResult.OK || btn.DialogResult == DialogResult.Yes)
            {
                btn.BackColor = ButtonBackground;
                btn.ForeColor = ButtonText;
            }
            else
            {
                btn.BackColor = ButtonSecondary;
                btn.ForeColor = TextPrimary;
            }
        }

        static void StyleTextBox(TextBox tb)
        {
            tb.BackColor = InputBackground;
            tb.ForeColor = TextPrimary;
            tb.BorderStyle = BorderStyle.FixedSingle;
            tb.Font = UIFont;
        }

        static void StyleLabel(Label lbl)
        {
            // Preserve custom ForeColor for strength labels, error text, etc.
            if (lbl.ForeColor == SystemColors.GrayText ||
                lbl.ForeColor == SystemColors.ControlText ||
                lbl.ForeColor == Color.Black)
            {
                lbl.ForeColor = TextPrimary;
            }
            lbl.BackColor = Color.Transparent;
        }

        static void StyleComboBox(ComboBox cb)
        {
            cb.BackColor = InputBackground;
            cb.ForeColor = TextPrimary;
            cb.FlatStyle = FlatStyle.Flat;
            cb.Font = UIFont;
        }

        /// <summary>Apply theme to MenuStrip.</summary>
        public static void ApplyToMenuStrip(MenuStrip menu)
        {
            menu.BackColor = MenuBackground;
            menu.ForeColor = MenuText;
            menu.Renderer = new DarkMenuRenderer();
        }

        /// <summary>Apply theme to StatusStrip.</summary>
        public static void ApplyToStatusStrip(StatusStrip status)
        {
            status.BackColor = StatusBackground;
            status.ForeColor = StatusText;
            status.Font = StatusFont;
            status.Renderer = new DarkStatusRenderer();
            foreach (ToolStripItem item in status.Items)
            {
                item.ForeColor = StatusText;
            }
        }

        /// <summary>Apply theme to ContextMenuStrip.</summary>
        public static void ApplyToContextMenu(ContextMenuStrip ctx)
        {
            ctx.BackColor = Surface;
            ctx.ForeColor = TextPrimary;
            ctx.Renderer = new DarkMenuRenderer();
        }

        // ── Custom renderers ──

        class DarkMenuRenderer : ToolStripProfessionalRenderer
        {
            public DarkMenuRenderer() : base(new DarkColorTable()) { }

            protected override void OnRenderItemText(ToolStripItemTextRenderEventArgs e)
            {
                e.TextColor = e.Item.Enabled ? MenuText : TextMuted;
                base.OnRenderItemText(e);
            }

            protected override void OnRenderMenuItemBackground(ToolStripItemRenderEventArgs e)
            {
                if (e.Item.Selected || e.Item.Pressed)
                {
                    using (var brush = new SolidBrush(MenuHover))
                    {
                        e.Graphics.FillRectangle(brush, new Rectangle(Point.Empty, e.Item.Size));
                    }
                }
                else
                {
                    using (var brush = new SolidBrush(MenuBackground))
                    {
                        e.Graphics.FillRectangle(brush, new Rectangle(Point.Empty, e.Item.Size));
                    }
                }
            }

            protected override void OnRenderSeparator(ToolStripSeparatorRenderEventArgs e)
            {
                int y = e.Item.Height / 2;
                using (var pen = new Pen(Border))
                {
                    e.Graphics.DrawLine(pen, 28, y, e.Item.Width - 4, y);
                }
            }

            protected override void OnRenderToolStripBackground(ToolStripRenderEventArgs e)
            {
                using (var brush = new SolidBrush(MenuBackground))
                {
                    e.Graphics.FillRectangle(brush, e.AffectedBounds);
                }
            }

            protected override void OnRenderToolStripBorder(ToolStripRenderEventArgs e)
            {
                using (var pen = new Pen(Border))
                {
                    e.Graphics.DrawRectangle(pen, 0, 0, e.AffectedBounds.Width - 1, e.AffectedBounds.Height - 1);
                }
            }

            protected override void OnRenderImageMargin(ToolStripRenderEventArgs e)
            {
                using (var brush = new SolidBrush(MenuBackground))
                {
                    e.Graphics.FillRectangle(brush, e.AffectedBounds);
                }
            }

            protected override void OnRenderArrow(ToolStripArrowRenderEventArgs e)
            {
                e.ArrowColor = TextSecondary;
                base.OnRenderArrow(e);
            }
        }

        class DarkColorTable : ProfessionalColorTable
        {
            public override Color MenuBorder { get { return Border; } }
            public override Color MenuItemBorder { get { return Color.Transparent; } }
            public override Color MenuItemSelected { get { return MenuHover; } }
            public override Color MenuStripGradientBegin { get { return MenuBackground; } }
            public override Color MenuStripGradientEnd { get { return MenuBackground; } }
            public override Color MenuItemSelectedGradientBegin { get { return MenuHover; } }
            public override Color MenuItemSelectedGradientEnd { get { return MenuHover; } }
            public override Color MenuItemPressedGradientBegin { get { return SurfaceLight; } }
            public override Color MenuItemPressedGradientEnd { get { return SurfaceLight; } }
            public override Color ToolStripDropDownBackground { get { return MenuBackground; } }
            public override Color ImageMarginGradientBegin { get { return MenuBackground; } }
            public override Color ImageMarginGradientMiddle { get { return MenuBackground; } }
            public override Color ImageMarginGradientEnd { get { return MenuBackground; } }
            public override Color SeparatorDark { get { return Border; } }
            public override Color SeparatorLight { get { return Border; } }
        }

        class DarkStatusRenderer : ToolStripProfessionalRenderer
        {
            protected override void OnRenderToolStripBackground(ToolStripRenderEventArgs e)
            {
                using (var brush = new SolidBrush(StatusBackground))
                {
                    e.Graphics.FillRectangle(brush, e.AffectedBounds);
                }
            }

            protected override void OnRenderToolStripBorder(ToolStripRenderEventArgs e)
            {
                // No border on status bar
            }
        }
    }
}
