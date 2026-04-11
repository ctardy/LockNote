using System;
using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    class SearchBar : Panel
    {
        TextBox txtSearch;
        LineNumberTextBox target;
        int lastIndex = -1;

        public SearchBar(LineNumberTextBox targetTextBox)
        {
            target = targetTextBox;
            Visible = false;
            Height = 32;
            Dock = DockStyle.Top;
            BackColor = Theme.Surface;

            var lbl = new Label { Text = "Find:", Location = new Point(8, 8), AutoSize = true, ForeColor = Theme.TextPrimary };
            txtSearch = new TextBox { Location = new Point(50, 5), Width = 240 };
            var btnNext = new Button { Text = "Next", Location = new Point(296, 4), Width = 60, Height = 24 };
            var btnClose = new Button { Text = "X", Location = new Point(362, 4), Width = 28, Height = 24 };

            Controls.AddRange(new Control[] { lbl, txtSearch, btnNext, btnClose });
            Theme.ApplyToControls(Controls);

            btnNext.Click += (s, e) => FindNext();
            txtSearch.KeyDown += (s, e) =>
            {
                if (e.KeyCode == Keys.Enter) { FindNext(); e.SuppressKeyPress = true; }
                if (e.KeyCode == Keys.Escape) { Hide(); target.Focus(); }
            };
            btnClose.Click += (s, e) => { Hide(); target.Focus(); };
        }

        public void ShowAndFocus()
        {
            lastIndex = -1;
            Visible = true;
            txtSearch.SelectAll();
            txtSearch.Focus();
        }

        void FindNext()
        {
            if (string.IsNullOrEmpty(txtSearch.Text)) return;

            int start = lastIndex + 1;
            if (start >= target.ContentLength) start = 0;

            int idx = target.ContentText.IndexOf(txtSearch.Text, start, StringComparison.OrdinalIgnoreCase);
            if (idx < 0 && start > 0)
                idx = target.ContentText.IndexOf(txtSearch.Text, 0, StringComparison.OrdinalIgnoreCase);

            if (idx >= 0)
            {
                target.SelectText(idx, txtSearch.Text.Length);
                target.ScrollToCaret();
                lastIndex = idx;
            }
            else
            {
                MessageBox.Show("Text not found.", "Find", MessageBoxButtons.OK, MessageBoxIcon.Information);
                lastIndex = -1;
            }
        }
    }
}
