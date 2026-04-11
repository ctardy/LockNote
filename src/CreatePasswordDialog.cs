using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    class CreatePasswordDialog : Form
    {
        TextBox txtPass, txtConfirm;
        public string Password { get; private set; }

        public CreatePasswordDialog()
        {
            Text = "LockNote - Create password";
            FormBorderStyle = FormBorderStyle.FixedDialog;
            StartPosition = FormStartPosition.CenterScreen;
            MaximizeBox = false;
            MinimizeBox = false;
            ClientSize = new Size(340, 160);

            var lbl1 = new Label { Text = "Password:", Location = new Point(12, 15), AutoSize = true };
            txtPass = new TextBox { Location = new Point(12, 35), Width = 310, PasswordChar = '*' };

            var lbl2 = new Label { Text = "Confirm:", Location = new Point(12, 65), AutoSize = true };
            txtConfirm = new TextBox { Location = new Point(12, 85), Width = 310, PasswordChar = '*' };

            var btnOK = new Button
            {
                Text = "OK",
                DialogResult = DialogResult.OK,
                Location = new Point(166, 120),
                Width = 75
            };
            var btnCancel = new Button
            {
                Text = "Cancel",
                DialogResult = DialogResult.Cancel,
                Location = new Point(247, 120),
                Width = 75
            };

            AcceptButton = btnOK;
            CancelButton = btnCancel;
            Controls.AddRange(new Control[] { lbl1, txtPass, lbl2, txtConfirm, btnOK, btnCancel });

            btnOK.Click += (s, e) =>
            {
                if (txtPass.Text.Length < 1)
                {
                    MessageBox.Show("Password cannot be empty.", "LockNote",
                        MessageBoxButtons.OK, MessageBoxIcon.Warning);
                    DialogResult = DialogResult.None;
                    return;
                }
                if (txtPass.Text != txtConfirm.Text)
                {
                    MessageBox.Show("Passwords do not match.", "LockNote",
                        MessageBoxButtons.OK, MessageBoxIcon.Warning);
                    DialogResult = DialogResult.None;
                    return;
                }
                Password = txtPass.Text;
            };
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                if (txtPass != null) txtPass.Clear();
                if (txtConfirm != null) txtConfirm.Clear();
            }
            base.Dispose(disposing);
        }
    }
}
