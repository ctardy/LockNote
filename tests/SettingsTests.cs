using System;

namespace LockNote.Tests
{
    static class SettingsTests
    {
        [Test]
        public static void ParseFrom_NoHeader_ReturnsDefaults()
        {
            string noteText;
            Settings s = Settings.ParseFrom("Hello world", out noteText);

            Assert.AreEqual(CloseAction.Ask, s.SaveOnClose, "Default should be Ask");
            Assert.AreEqual("Hello world", noteText, "Note text should be unchanged");
        }

        [Test]
        public static void ParseFrom_Null_ReturnsDefaults()
        {
            string noteText;
            Settings s = Settings.ParseFrom(null, out noteText);

            Assert.AreEqual(CloseAction.Ask, s.SaveOnClose);
            Assert.AreEqual("", noteText, "Null input should produce empty note");
        }

        [Test]
        public static void ParseFrom_EmptyString_ReturnsDefaults()
        {
            string noteText;
            Settings s = Settings.ParseFrom("", out noteText);

            Assert.AreEqual(CloseAction.Ask, s.SaveOnClose);
            Assert.AreEqual("", noteText);
        }

        [Test]
        public static void ParseFrom_WithHeader_ParsesSettings()
        {
            string raw = "[LOCKNOTE_SETTINGS]\r\nsave_on_close=always\r\n[/LOCKNOTE_SETTINGS]\r\nMy note";
            string noteText;
            Settings s = Settings.ParseFrom(raw, out noteText);

            Assert.AreEqual(CloseAction.Always, s.SaveOnClose);
            Assert.AreEqual("My note", noteText);
        }

        [Test]
        public static void ParseFrom_NeverSetting()
        {
            string raw = "[LOCKNOTE_SETTINGS]\nsave_on_close=never\n[/LOCKNOTE_SETTINGS]\nContent";
            string noteText;
            Settings s = Settings.ParseFrom(raw, out noteText);

            Assert.AreEqual(CloseAction.Never, s.SaveOnClose);
            Assert.AreEqual("Content", noteText);
        }

        [Test]
        public static void ParseFrom_UnknownValue_DefaultsToAsk()
        {
            string raw = "[LOCKNOTE_SETTINGS]\nsave_on_close=banana\n[/LOCKNOTE_SETTINGS]\nText";
            string noteText;
            Settings s = Settings.ParseFrom(raw, out noteText);

            Assert.AreEqual(CloseAction.Ask, s.SaveOnClose, "Unknown value should default to Ask");
        }

        [Test]
        public static void ParseFrom_UnknownKey_Ignored()
        {
            string raw = "[LOCKNOTE_SETTINGS]\nfuture_setting=yes\nsave_on_close=always\n[/LOCKNOTE_SETTINGS]\nText";
            string noteText;
            Settings s = Settings.ParseFrom(raw, out noteText);

            Assert.AreEqual(CloseAction.Always, s.SaveOnClose, "Known keys should still parse");
            Assert.AreEqual("Text", noteText);
        }

        [Test]
        public static void PrependTo_RoundTrip_Ask()
        {
            var s = new Settings();
            s.SaveOnClose = CloseAction.Ask;
            string combined = s.PrependTo("My note content");

            string noteText;
            Settings parsed = Settings.ParseFrom(combined, out noteText);

            Assert.AreEqual(CloseAction.Ask, parsed.SaveOnClose);
            Assert.AreEqual("My note content", noteText);
        }

        [Test]
        public static void PrependTo_RoundTrip_Always()
        {
            var s = new Settings();
            s.SaveOnClose = CloseAction.Always;
            string combined = s.PrependTo("Content here");

            string noteText;
            Settings parsed = Settings.ParseFrom(combined, out noteText);

            Assert.AreEqual(CloseAction.Always, parsed.SaveOnClose);
            Assert.AreEqual("Content here", noteText);
        }

        [Test]
        public static void PrependTo_RoundTrip_Never()
        {
            var s = new Settings();
            s.SaveOnClose = CloseAction.Never;
            string combined = s.PrependTo("Other text");

            string noteText;
            Settings parsed = Settings.ParseFrom(combined, out noteText);

            Assert.AreEqual(CloseAction.Never, parsed.SaveOnClose);
            Assert.AreEqual("Other text", noteText);
        }

        [Test]
        public static void PrependTo_RoundTrip_EmptyNote()
        {
            var s = new Settings();
            s.SaveOnClose = CloseAction.Always;
            string combined = s.PrependTo("");

            string noteText;
            Settings parsed = Settings.ParseFrom(combined, out noteText);

            Assert.AreEqual(CloseAction.Always, parsed.SaveOnClose);
            Assert.AreEqual("", noteText);
        }

        [Test]
        public static void PrependTo_RoundTrip_MultilineNote()
        {
            var s = new Settings();
            s.SaveOnClose = CloseAction.Ask;
            string original = "Line 1\r\nLine 2\r\nLine 3";
            string combined = s.PrependTo(original);

            string noteText;
            Settings parsed = Settings.ParseFrom(combined, out noteText);

            Assert.AreEqual(original, noteText, "Multiline note should survive round-trip");
        }

        [Test]
        public static void FullCryptoRoundTrip_WithSettings()
        {
            // Settings + note -> prepend -> encrypt -> decrypt -> parse -> verify
            var s = new Settings();
            s.SaveOnClose = CloseAction.Never;
            string note = "Secret note content";
            string password = "testpwd";

            string payload = s.PrependTo(note);
            byte[] encrypted = Crypto.Encrypt(payload, password);
            string decrypted = Crypto.Decrypt(encrypted, password);

            string noteText;
            Settings parsed = Settings.ParseFrom(decrypted, out noteText);

            Assert.AreEqual(CloseAction.Never, parsed.SaveOnClose);
            Assert.AreEqual(note, noteText, "Full crypto round-trip with settings");
        }
    }
}
