using System;
using System.Collections.Generic;

namespace LockNote.Tests
{
    static class TabStoreTests
    {
        [Test]
        public static void Parse_Legacy_SingleTab()
        {
            List<NoteTab> tabs = TabStore.Parse("Hello world");
            Assert.AreEqual(1, tabs.Count);
            Assert.AreEqual("Note 1", tabs[0].Name);
            Assert.AreEqual("Hello world", tabs[0].Content);
        }

        [Test]
        public static void Parse_Null_SingleEmptyTab()
        {
            List<NoteTab> tabs = TabStore.Parse(null);
            Assert.AreEqual(1, tabs.Count);
            Assert.AreEqual("Note 1", tabs[0].Name);
            Assert.AreEqual("", tabs[0].Content);
        }

        [Test]
        public static void Parse_EmptyString_SingleEmptyTab()
        {
            List<NoteTab> tabs = TabStore.Parse("");
            Assert.AreEqual(1, tabs.Count);
            Assert.AreEqual("Note 1", tabs[0].Name);
            Assert.AreEqual("", tabs[0].Content);
        }

        [Test]
        public static void Parse_SingleTab_CorrectNameAndContent()
        {
            string input = "[LOCKNOTE_TABS]\r\n[TAB:My Tab]\r\nSome content here\r\n[/LOCKNOTE_TABS]";
            List<NoteTab> tabs = TabStore.Parse(input);
            Assert.AreEqual(1, tabs.Count);
            Assert.AreEqual("My Tab", tabs[0].Name);
            Assert.AreEqual("Some content here", tabs[0].Content);
        }

        [Test]
        public static void Parse_MultipleTabs_CorrectNamesAndContents()
        {
            string input = "[LOCKNOTE_TABS]\r\n[TAB:Tab A]\r\nContent A\r\n[TAB:Tab B]\r\nContent B\r\n[/LOCKNOTE_TABS]";
            List<NoteTab> tabs = TabStore.Parse(input);
            Assert.AreEqual(2, tabs.Count);
            Assert.AreEqual("Tab A", tabs[0].Name);
            Assert.AreEqual("Content A", tabs[0].Content);
            Assert.AreEqual("Tab B", tabs[1].Name);
            Assert.AreEqual("Content B", tabs[1].Content);
        }

        [Test]
        public static void Parse_ContentWithTabMarker_EscapingWorks()
        {
            // Content that literally contains [TAB: should be escaped during serialize
            var tabs = new List<NoteTab>();
            tabs.Add(new NoteTab("Test", "[TAB:fake marker]"));
            string serialized = TabStore.Serialize(tabs);
            List<NoteTab> parsed = TabStore.Parse(serialized);
            Assert.AreEqual(1, parsed.Count);
            Assert.AreEqual("Test", parsed[0].Name);
            Assert.AreEqual("[TAB:fake marker]", parsed[0].Content);
        }

        [Test]
        public static void RoundTrip_SingleTab()
        {
            var tabs = new List<NoteTab>();
            tabs.Add(new NoteTab("Note 1", "Hello world"));
            string serialized = TabStore.Serialize(tabs);
            List<NoteTab> parsed = TabStore.Parse(serialized);
            Assert.AreEqual(1, parsed.Count);
            Assert.AreEqual("Note 1", parsed[0].Name);
            Assert.AreEqual("Hello world", parsed[0].Content);
        }

        [Test]
        public static void RoundTrip_MultipleTabs()
        {
            var tabs = new List<NoteTab>();
            tabs.Add(new NoteTab("First", "Content 1\nLine 2"));
            tabs.Add(new NoteTab("Second", "Content 2"));
            tabs.Add(new NoteTab("Third", "Content 3\nLine B\nLine C"));
            string serialized = TabStore.Serialize(tabs);
            List<NoteTab> parsed = TabStore.Parse(serialized);
            Assert.AreEqual(3, parsed.Count);
            Assert.AreEqual("First", parsed[0].Name);
            Assert.AreEqual("Content 1\nLine 2", parsed[0].Content);
            Assert.AreEqual("Second", parsed[1].Name);
            Assert.AreEqual("Content 2", parsed[1].Content);
            Assert.AreEqual("Third", parsed[2].Name);
            Assert.AreEqual("Content 3\nLine B\nLine C", parsed[2].Content);
        }

        [Test]
        public static void RoundTrip_EmptyContent()
        {
            var tabs = new List<NoteTab>();
            tabs.Add(new NoteTab("Empty", ""));
            string serialized = TabStore.Serialize(tabs);
            List<NoteTab> parsed = TabStore.Parse(serialized);
            Assert.AreEqual(1, parsed.Count);
            Assert.AreEqual("Empty", parsed[0].Name);
            Assert.AreEqual("", parsed[0].Content);
        }

        [Test]
        public static void RoundTrip_ContentWithDelimiters()
        {
            var tabs = new List<NoteTab>();
            tabs.Add(new NoteTab("Special", "[LOCKNOTE_TABS]\n[TAB:not a tab]\n[/LOCKNOTE_TABS]"));
            string serialized = TabStore.Serialize(tabs);
            List<NoteTab> parsed = TabStore.Parse(serialized);
            Assert.AreEqual(1, parsed.Count);
            Assert.AreEqual("Special", parsed[0].Name);
            Assert.AreEqual("[LOCKNOTE_TABS]\n[TAB:not a tab]\n[/LOCKNOTE_TABS]", parsed[0].Content);
        }

        [Test]
        public static void NextTabNumber_NoNoteTabs_Returns1()
        {
            var tabs = new List<NoteTab>();
            tabs.Add(new NoteTab("My Custom Tab", ""));
            tabs.Add(new NoteTab("Another", ""));
            Assert.AreEqual(1, TabStore.NextTabNumber(tabs));
        }

        [Test]
        public static void NextTabNumber_WithGaps_ReturnsMaxPlus1()
        {
            var tabs = new List<NoteTab>();
            tabs.Add(new NoteTab("Note 1", ""));
            tabs.Add(new NoteTab("Note 3", ""));
            Assert.AreEqual(4, TabStore.NextTabNumber(tabs));
        }

        [Test]
        public static void NextTabNumber_EmptyList_Returns1()
        {
            var tabs = new List<NoteTab>();
            Assert.AreEqual(1, TabStore.NextTabNumber(tabs));
        }

        [Test]
        public static void NoteTab_Modified_FlagWorks()
        {
            var tab = new NoteTab("Test", "original");
            Assert.IsFalse(tab.Modified, "New tab should not be modified");
            tab.Content = "changed";
            Assert.IsTrue(tab.Modified, "Tab should be modified after content change");
            tab.Content = "original";
            Assert.IsFalse(tab.Modified, "Tab should not be modified after reverting content");
        }

        [Test]
        public static void NoteTab_MarkClean_ResetsModified()
        {
            var tab = new NoteTab("Test", "original");
            tab.Content = "changed";
            Assert.IsTrue(tab.Modified);
            tab.MarkClean();
            Assert.IsFalse(tab.Modified, "MarkClean should reset Modified flag");
            Assert.AreEqual("changed", tab.SavedContent);
        }

        [Test]
        public static void Settings_ActiveTab_RoundTrip()
        {
            var s = new Settings();
            s.ActiveTab = 3;
            string combined = s.PrependTo("test content");

            string noteText;
            Settings parsed = Settings.ParseFrom(combined, out noteText);

            Assert.AreEqual(3, parsed.ActiveTab, "ActiveTab should survive round-trip");
            Assert.AreEqual("test content", noteText);
        }

        [Test]
        public static void Settings_ActiveTab_DefaultsToZero()
        {
            var s = new Settings();
            Assert.AreEqual(0, s.ActiveTab, "ActiveTab should default to 0");
        }

        [Test]
        public static void Settings_ActiveTab_ParsedFromHeader()
        {
            string raw = "[LOCKNOTE_SETTINGS]\r\nactive_tab=2\r\nsave_on_close=ask\r\n[/LOCKNOTE_SETTINGS]\r\nContent";
            string noteText;
            Settings parsed = Settings.ParseFrom(raw, out noteText);
            Assert.AreEqual(2, parsed.ActiveTab);
        }
    }
}
