using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    class RenameTabDialog : Form
    {
        TextBox txtName;
        public string TabName { get; private set; }

        public RenameTabDialog(string currentName)
        {
            Text = "Rename tab";
            FormBorderStyle = FormBorderStyle.FixedDialog;
            StartPosition = FormStartPosition.CenterParent;
            MaximizeBox = false;
            MinimizeBox = false;
            ClientSize = new Size(300, 100);

            var lbl = new Label { Text = "Tab name:", Location = new Point(20, 15), AutoSize = true };
            txtName = new TextBox { Location = new Point(20, 38), Width = 260, Text = currentName };
            txtName.SelectAll();

            var btnOK = new Button
            {
                Text = "OK",
                DialogResult = DialogResult.OK,
                Location = new Point(120, 68),
                Width = 75,
                Height = 28
            };
            var btnCancel = new Button
            {
                Text = "Cancel",
                DialogResult = DialogResult.Cancel,
                Location = new Point(201, 68),
                Width = 75,
                Height = 28
            };

            AcceptButton = btnOK;
            CancelButton = btnCancel;
            Controls.AddRange(new Control[] { lbl, txtName, btnOK, btnCancel });

            btnOK.Click += delegate
            {
                string name = txtName.Text.Trim();
                if (name.Length == 0)
                {
                    MessageBox.Show("Tab name cannot be empty.", "Rename tab",
                        MessageBoxButtons.OK, MessageBoxIcon.Warning);
                    DialogResult = DialogResult.None;
                    return;
                }
                TabName = name;
            };

            Theme.ApplyToDialog(this);
        }
    }
}
