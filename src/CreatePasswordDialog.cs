using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    class CreatePasswordDialog : Form
    {
        TextBox txtPass, txtConfirm;
        Panel strengthBar;
        Label strengthLabel;
        public string Password { get; private set; }

        public CreatePasswordDialog()
        {
            Text = "LockNote - Create password";
            FormBorderStyle = FormBorderStyle.FixedDialog;
            StartPosition = FormStartPosition.CenterScreen;
            MaximizeBox = false;
            MinimizeBox = false;
            ClientSize = new Size(340, 165);

            var lbl1 = new Label { Text = "Password:", Location = new Point(12, 15), AutoSize = true };
            txtPass = new TextBox { Location = new Point(12, 35), Width = 310, PasswordChar = '*' };

            strengthBar = new Panel { Location = new Point(12, 58), Size = new Size(0, 4), BackColor = Color.Transparent };
            strengthLabel = new Label { Location = new Point(12, 55), Width = 310, AutoSize = false, TextAlign = ContentAlignment.TopRight, ForeColor = SystemColors.GrayText, Text = "" };

            var lbl2 = new Label { Text = "Confirm:", Location = new Point(12, 72), AutoSize = true };
            txtConfirm = new TextBox { Location = new Point(12, 92), Width = 310, PasswordChar = '*' };

            var btnOK = new Button
            {
                Text = "OK",
                DialogResult = DialogResult.OK,
                Location = new Point(166, 127),
                Width = 75
            };
            var btnCancel = new Button
            {
                Text = "Cancel",
                DialogResult = DialogResult.Cancel,
                Location = new Point(247, 127),
                Width = 75
            };

            AcceptButton = btnOK;
            CancelButton = btnCancel;
            Controls.AddRange(new Control[] { lbl1, txtPass, strengthBar, strengthLabel, lbl2, txtConfirm, btnOK, btnCancel });

            txtPass.TextChanged += delegate { UpdateStrengthIndicator(); };

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

        void UpdateStrengthIndicator()
        {
            int score = ScorePassword(txtPass.Text);
            string text;
            Color color;
            int barWidth;
            if (score <= 1)
            {
                text = "Weak";
                color = ColorTranslator.FromHtml("#D32F2F");
                barWidth = 310 / 4;
            }
            else if (score <= 3)
            {
                text = "Fair";
                color = ColorTranslator.FromHtml("#F57C00");
                barWidth = 310 / 2;
            }
            else if (score <= 5)
            {
                text = "Strong";
                color = ColorTranslator.FromHtml("#689F38");
                barWidth = 310 * 3 / 4;
            }
            else
            {
                text = "Very strong";
                color = ColorTranslator.FromHtml("#388E3C");
                barWidth = 310;
            }

            if (string.IsNullOrEmpty(txtPass.Text))
            {
                strengthBar.Width = 0;
                strengthLabel.Text = "";
                return;
            }

            strengthBar.Width = barWidth;
            strengthBar.BackColor = color;
            strengthLabel.Text = text;
            strengthLabel.ForeColor = color;
        }

        static int ScorePassword(string pwd)
        {
            if (string.IsNullOrEmpty(pwd)) return 0;
            int score = 0;
            if (pwd.Length >= 8) score++;
            if (pwd.Length >= 12) score++;
            if (pwd.Length >= 16) score++;
            bool hasLower = false, hasUpper = false, hasDigit = false, hasSymbol = false;
            for (int i = 0; i < pwd.Length; i++)
            {
                char c = pwd[i];
                if (c >= 'a' && c <= 'z') hasLower = true;
                else if (c >= 'A' && c <= 'Z') hasUpper = true;
                else if (c >= '0' && c <= '9') hasDigit = true;
                else hasSymbol = true;
            }
            if (hasLower) score++;
            if (hasUpper) score++;
            if (hasDigit) score++;
            if (hasSymbol) score++;
            return score;
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
