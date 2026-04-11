using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    class CreatePasswordDialog : Form
    {
        TextBox txtPass, txtConfirm;
        Panel strengthBarBg;
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
            ClientSize = new Size(380, 195);

            var lbl1 = new Label { Text = "Password:", Location = new Point(20, 18), AutoSize = true };
            txtPass = new TextBox { Location = new Point(20, 40), Width = 340, PasswordChar = '*' };

            // Strength bar background (track)
            strengthBarBg = new Panel
            {
                Location = new Point(20, 66),
                Size = new Size(250, 4),
                BackColor = Theme.Border
            };
            // Strength bar foreground (fill)
            strengthBar = new Panel
            {
                Location = new Point(0, 0),
                Size = new Size(0, 4),
                BackColor = Color.Transparent
            };
            strengthBarBg.Controls.Add(strengthBar);

            strengthLabel = new Label
            {
                Location = new Point(275, 61),
                Width = 85,
                AutoSize = false,
                TextAlign = ContentAlignment.TopRight,
                ForeColor = Theme.TextSecondary,
                Text = ""
            };

            var lbl2 = new Label { Text = "Confirm:", Location = new Point(20, 82), AutoSize = true };
            txtConfirm = new TextBox { Location = new Point(20, 104), Width = 340, PasswordChar = '*' };

            var btnOK = new Button
            {
                Text = "Create",
                DialogResult = DialogResult.OK,
                Location = new Point(196, 148),
                Width = 80,
                Height = 30
            };
            var btnCancel = new Button
            {
                Text = "Cancel",
                DialogResult = DialogResult.Cancel,
                Location = new Point(282, 148),
                Width = 80,
                Height = 30
            };

            AcceptButton = btnOK;
            CancelButton = btnCancel;
            Controls.AddRange(new Control[] {
                lbl1, txtPass, strengthBarBg, strengthLabel,
                lbl2, txtConfirm, btnOK, btnCancel
            });

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

            Theme.ApplyToDialog(this);
        }

        void UpdateStrengthIndicator()
        {
            int score = ScorePassword(txtPass.Text);
            string text;
            Color color;
            int barWidth;
            int maxWidth = strengthBarBg.Width;

            if (score <= 1)
            {
                text = "Weak";
                color = Color.FromArgb(244, 67, 54);
                barWidth = maxWidth / 4;
            }
            else if (score <= 3)
            {
                text = "Fair";
                color = Color.FromArgb(255, 152, 0);
                barWidth = maxWidth / 2;
            }
            else if (score <= 5)
            {
                text = "Strong";
                color = Color.FromArgb(139, 195, 74);
                barWidth = maxWidth * 3 / 4;
            }
            else
            {
                text = "Very strong";
                color = Color.FromArgb(76, 175, 80);
                barWidth = maxWidth;
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
