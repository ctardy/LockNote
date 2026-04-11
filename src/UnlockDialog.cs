using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    class UnlockDialog : Form
    {
        TextBox txtPass;
        Label lblAttempts;
        byte[] encryptedData;
        int attempts;
        const int MaxAttempts = 5;

        public string Password { get; private set; }
        public string DecryptedText { get; private set; }

        public UnlockDialog(byte[] data)
        {
            encryptedData = data;

            Text = "LockNote - Unlock";
            FormBorderStyle = FormBorderStyle.FixedDialog;
            StartPosition = FormStartPosition.CenterScreen;
            MaximizeBox = false;
            MinimizeBox = false;
            ClientSize = new Size(340, 120);

            var lbl = new Label { Text = "Password:", Location = new Point(12, 15), AutoSize = true };
            txtPass = new TextBox { Location = new Point(12, 35), Width = 310, PasswordChar = '*' };
            lblAttempts = new Label { Text = "", Location = new Point(12, 65), AutoSize = true, ForeColor = Color.Red };

            var btnOK = new Button
            {
                Text = "OK",
                DialogResult = DialogResult.OK,
                Location = new Point(166, 85),
                Width = 75
            };
            var btnCancel = new Button
            {
                Text = "Cancel",
                DialogResult = DialogResult.Cancel,
                Location = new Point(247, 85),
                Width = 75
            };

            AcceptButton = btnOK;
            CancelButton = btnCancel;
            Controls.AddRange(new Control[] { lbl, txtPass, lblAttempts, btnOK, btnCancel });

            btnOK.Click += (s, e) =>
            {
                string result = Crypto.Decrypt(encryptedData, txtPass.Text);
                if (result == null)
                {
                    attempts++;
                    if (attempts >= MaxAttempts)
                    {
                        MessageBox.Show("Maximum number of attempts reached.", "LockNote",
                            MessageBoxButtons.OK, MessageBoxIcon.Error);
                        DialogResult = DialogResult.Cancel;
                        return;
                    }
                    lblAttempts.Text = string.Format("Wrong password ({0}/{1})", attempts, MaxAttempts);
                    DialogResult = DialogResult.None;
                    txtPass.SelectAll();
                    txtPass.Focus();
                    return;
                }
                Password = txtPass.Text;
                DecryptedText = result;
            };
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing && txtPass != null) txtPass.Clear();
            base.Dispose(disposing);
        }
    }
}
