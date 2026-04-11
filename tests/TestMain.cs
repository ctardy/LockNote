using System;

namespace LockNote.Tests
{
    class TestMain
    {
        static int Main()
        {
            return TestRunner.Run(new Type[]
            {
                typeof(CryptoTests),
                typeof(SettingsTests),
                typeof(StorageTests),
                typeof(TabStoreTests)
            });
        }
    }
}
