using System;
using System.ComponentModel;
using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    class UnlockDialog : Form
    {
        TextBox txtPass;
        Label lblAttempts;
        Button btnOK;
        Button btnCancel;
        byte[] encryptedData;
        int attempts;
        const int MaxAttempts = 5;
        BackgroundWorker worker;

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
            ClientSize = new Size(340, 130);

            var lbl = new Label { Text = "Password:", Location = new Point(20, 18), AutoSize = true };
            txtPass = new TextBox { Location = new Point(20, 40), Width = 300, PasswordChar = '*' };
            lblAttempts = new Label { Text = "", Location = new Point(20, 68), AutoSize = true, ForeColor = Theme.ErrorText };

            btnOK = new Button
            {
                Text = "Unlock",
                DialogResult = DialogResult.None,
                Location = new Point(164, 90),
                Width = 80,
                Height = 30
            };
            btnCancel = new Button
            {
                Text = "Cancel",
                DialogResult = DialogResult.Cancel,
                Location = new Point(250, 90),
                Width = 80,
                Height = 30
            };

            AcceptButton = btnOK;
            CancelButton = btnCancel;
            Controls.AddRange(new Control[] { lbl, txtPass, lblAttempts, btnOK, btnCancel });

            worker = new BackgroundWorker();
            worker.DoWork += OnDecryptWork;
            worker.RunWorkerCompleted += OnDecryptCompleted;

            btnOK.Click += (s, e) => StartDecrypt();

            Theme.ApplyToDialog(this);
        }

        void StartDecrypt()
        {
            if (worker.IsBusy) return;

            string pwd = txtPass.Text;
            if (pwd.Length == 0)
            {
                lblAttempts.Text = "Password cannot be empty.";
                return;
            }

            btnOK.Enabled = false;
            txtPass.Enabled = false;
            lblAttempts.ForeColor = Theme.TextSecondary;
            lblAttempts.Text = "Decrypting...";
            Cursor = Cursors.WaitCursor;

            worker.RunWorkerAsync(pwd);
        }

        void OnDecryptWork(object sender, DoWorkEventArgs e)
        {
            string pwd = (string)e.Argument;
            e.Result = Crypto.Decrypt(encryptedData, pwd);
        }

        void OnDecryptCompleted(object sender, RunWorkerCompletedEventArgs e)
        {
            Cursor = Cursors.Default;
            btnOK.Enabled = true;
            txtPass.Enabled = true;
            lblAttempts.ForeColor = Theme.ErrorText;

            string result = (string)e.Result;

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
                txtPass.SelectAll();
                txtPass.Focus();
                return;
            }

            Password = txtPass.Text;
            DecryptedText = result;
            DialogResult = DialogResult.OK;
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                if (txtPass != null) txtPass.Clear();
                if (worker != null) worker.Dispose();
            }
            base.Dispose(disposing);
        }
    }
}
