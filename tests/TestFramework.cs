using System;
using System.Collections.Generic;
using System.Reflection;

namespace LockNote.Tests
{
    /// <summary>
    /// Minimal test framework — no NuGet, no dependencies.
    /// Tests are methods marked with [Test] that throw on failure.
    /// </summary>
    [AttributeUsage(AttributeTargets.Method)]
    class TestAttribute : Attribute { }

    static class Assert
    {
        public static void IsTrue(bool condition, string message = null)
        {
            if (!condition)
                throw new Exception("Assert.IsTrue failed" + (message != null ? ": " + message : ""));
        }

        public static void IsFalse(bool condition, string message = null)
        {
            if (condition)
                throw new Exception("Assert.IsFalse failed" + (message != null ? ": " + message : ""));
        }

        public static void AreEqual(object expected, object actual, string message = null)
        {
            if (!object.Equals(expected, actual))
                throw new Exception(string.Format("Assert.AreEqual failed: expected <{0}> but got <{1}>{2}",
                    expected, actual, message != null ? " — " + message : ""));
        }

        public static void AreNotEqual(object expected, object actual, string message = null)
        {
            if (object.Equals(expected, actual))
                throw new Exception(string.Format("Assert.AreNotEqual failed: both are <{0}>{1}",
                    expected, message != null ? " — " + message : ""));
        }

        public static void IsNull(object value, string message = null)
        {
            if (value != null)
                throw new Exception("Assert.IsNull failed" + (message != null ? ": " + message : ""));
        }

        public static void IsNotNull(object value, string message = null)
        {
            if (value == null)
                throw new Exception("Assert.IsNotNull failed" + (message != null ? ": " + message : ""));
        }

        public static void Throws<T>(Action action, string message = null) where T : Exception
        {
            try
            {
                action();
            }
            catch (T)
            {
                return; // expected
            }
            catch (Exception ex)
            {
                throw new Exception(string.Format("Assert.Throws<{0}> failed: got {1} instead{2}",
                    typeof(T).Name, ex.GetType().Name, message != null ? " — " + message : ""));
            }
            throw new Exception(string.Format("Assert.Throws<{0}> failed: no exception thrown{1}",
                typeof(T).Name, message != null ? " — " + message : ""));
        }
    }

    static class TestRunner
    {
        public static int Run(Type[] testClasses)
        {
            int passed = 0;
            int failed = 0;
            var failures = new List<string>();

            foreach (Type t in testClasses)
            {
                Console.WriteLine("--- {0} ---", t.Name);
                MethodInfo[] methods = t.GetMethods(BindingFlags.Public | BindingFlags.Static);

                foreach (MethodInfo m in methods)
                {
                    object[] attrs = m.GetCustomAttributes(typeof(TestAttribute), false);
                    if (attrs.Length == 0) continue;

                    string name = t.Name + "." + m.Name;
                    try
                    {
                        m.Invoke(null, null);
                        Console.WriteLine("  PASS  {0}", m.Name);
                        passed++;
                    }
                    catch (TargetInvocationException tie)
                    {
                        Console.WriteLine("  FAIL  {0}: {1}", m.Name, tie.InnerException.Message);
                        failures.Add(name + ": " + tie.InnerException.Message);
                        failed++;
                    }
                    catch (Exception ex)
                    {
                        Console.WriteLine("  FAIL  {0}: {1}", m.Name, ex.Message);
                        failures.Add(name + ": " + ex.Message);
                        failed++;
                    }
                }
            }

            Console.WriteLine();
            Console.WriteLine("========================================");
            Console.WriteLine("  {0} passed, {1} failed, {2} total", passed, failed, passed + failed);
            Console.WriteLine("========================================");

            if (failures.Count > 0)
            {
                Console.WriteLine();
                Console.WriteLine("Failures:");
                foreach (string f in failures)
                    Console.WriteLine("  - " + f);
            }

            return failed > 0 ? 1 : 0;
        }
    }
}
