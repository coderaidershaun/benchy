// Runs a whole session through the program's line handling and returns its output as text.
using System.Text;
using Engine;

namespace Engine.Tests;

internal static class Sessions
{
    public static byte[] Run(byte[] input)
    {
        using var stdin = new MemoryStream(input);
        using var stdout = new MemoryStream();
        Session.Run(stdin, stdout);
        return stdout.ToArray();
    }

    public static string Run(string input) => Encoding.ASCII.GetString(Run(Encoding.ASCII.GetBytes(input)));

    public static string Lines(params string[] lines) => string.Concat(lines.Select(line => line + "\n"));
}
