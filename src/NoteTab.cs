using System;

namespace LockNote
{
    class NoteTab
    {
        public string Name { get; set; }
        public string Content { get; set; }
        public string SavedContent { get; set; }

        public bool Modified
        {
            get { return Content != SavedContent; }
        }

        public NoteTab(string name, string content)
        {
            Name = name;
            Content = content;
            SavedContent = content;
        }

        public void MarkClean()
        {
            SavedContent = Content;
        }
    }
}
