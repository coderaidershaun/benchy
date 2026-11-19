// The public example sessions must produce their expected output byte for byte.
namespace Engine.Tests;

public class ExampleTests
{
    public static TheoryData<string> Names() =>
        new(Directory.GetFiles(Path.Combine(AppContext.BaseDirectory, "Examples"), "*.in").Select(Path.GetFileNameWithoutExtension).OfType<string>());

    [Theory]
    [MemberData(nameof(Names))]
    public void ExampleOutputMatchesExactly(string name)
    {
        string directory = Path.Combine(AppContext.BaseDirectory, "Examples");
        byte[] expected = File.ReadAllBytes(Path.Combine(directory, name + ".out"));

        byte[] actual = Sessions.Run(File.ReadAllBytes(Path.Combine(directory, name + ".in")));

        Assert.Equal(System.Text.Encoding.ASCII.GetString(expected), System.Text.Encoding.ASCII.GetString(actual));
    }

    [Fact]
    public void AllNineExamplesAreFound() => Assert.Equal(9, Names().Count);
}
