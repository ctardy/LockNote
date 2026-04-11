using System;
using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    class RenameTabDialog : Form
    {
        TextBox txtName;
        Label lblError;

        public string TabName { get; private set; }

        public RenameTabDialog(string currentName)
        {
            Text = "Rename Tab";
            FormBorderStyle = FormBorderStyle.FixedDialog;
            StartPosition = FormStartPosition.CenterParent;
            MaximizeBox = false;
            MinimizeBox = false;
            ClientSize = new Size(340, 120);

            var lbl = new Label
            {
                Text = "Tab name:",
                Location = new Point(20, 18),
                AutoSize = true
            };

            txtName = new TextBox
            {
                Text = currentName,
                Location = new Point(20, 40),
                Width = 300
            };
            txtName.SelectAll();

            lblError = new Label
            {
                Text = "",
                Location = new Point(20, 65),
                AutoSize = true,
                ForeColor = Color.Red
            };

            var btnOk = new Button
            {
                Text = "OK",
                DialogResult = DialogResult.OK,
                Location = new Point(154, 82),
                Width = 80,
                Height = 28
            };

            var btnCancel = new Button
            {
                Text = "Cancel",
                DialogResult = DialogResult.Cancel,
                Location = new Point(240, 82),
                Width = 80,
                Height = 28
            };

            AcceptButton = btnOk;
            CancelButton = btnCancel;
            Controls.AddRange(new Control[] { lbl, txtName, lblError, btnOk, btnCancel });

            Theme.ApplyToDialog(this);

            btnOk.Click += delegate
            {
                string name = txtName.Text.Trim();
                if (name.Length == 0)
                {
                    lblError.Text = "Name cannot be empty.";
                    lblError.ForeColor = Theme.ErrorText;
                    DialogResult = DialogResult.None;
                    return;
                }
                TabName = name;
            };
        }
    }
}
