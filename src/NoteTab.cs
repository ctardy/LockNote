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

        public NoteTab()
        {
            Name = "Untitled";
            Content = "";
            SavedContent = "";
        }
    }
}
