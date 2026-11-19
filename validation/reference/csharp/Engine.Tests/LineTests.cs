// How input bytes become numbered lines, and how a line is rejected before the engine state is read.
namespace Engine.Tests;

public class LineTests
{
    [Fact]
    public void CrBeforeLfIsRemovedAlsoOnAFinalLineWithoutLf() =>
        Assert.Equal("TOP,ACME,0,0,0,0\nTOP,ACME,0,0,0,0\n", Sessions.Run("S,ACME,1,1\r\nQ,ACME\r\nQ,ACME\r").Replace("SYMBOL,ACME\n", "", StringComparison.Ordinal));

    [Fact]
    public void OnlyOneCrIsRemoved() =>
        Assert.Equal("SYMBOL,ACME\nREJECT,2,MALFORMED\n", Sessions.Run("S,ACME,1,1\nQ,ACME\r\r\n"));

    [Fact]
    public void EmptyLinesCountAndInputEndingInLfHasNoExtraLine() =>
        Assert.Equal("REJECT,1,UNKNOWN_COMMAND\nREJECT,2,UNKNOWN_COMMAND\nREJECT,3,UNKNOWN_SYMBOL\n", Sessions.Run("\n\nQ,ACME\n"));

    [Theory]
    [InlineData(256, "REJECT,1,UNKNOWN_COMMAND\n")]
    [InlineData(257, "REJECT,1,MALFORMED\n")]
    [InlineData(100_000, "REJECT,1,MALFORMED\n")]
    public void LengthIsCheckedBeforeAnythingElse(int length, string expected) =>
        Assert.Equal(expected, Sessions.Run(new string('X', length) + "\n"));

    [Fact]
    public void A257ByteLineEndingInCrIsNotTooLong() =>
        Assert.Equal("REJECT,1,UNKNOWN_COMMAND\n", Sessions.Run(new string('X', 256) + "\r\n"));

    [Theory]
    [InlineData("N,1,ACME,B,000000000000000005,10,GTC,AL", "ACCEPT,1\n")]
    [InlineData("N,1,ACME,B,0000000000000000005,10,GTC,AL", "REJECT,2,MALFORMED\n")]
    [InlineData("N,1,ACME,B,1000000001,10,GTC,AL", "REJECT,2,MALFORMED\n")]
    [InlineData("N,999999999999999999,ACME,B,5,10,GTC,AL", "ACCEPT,999999999999999999\n")]
    [InlineData("N,1,ACME,B,5,10,GTC,ABCDEFGHI", "REJECT,2,MALFORMED\n")]
    [InlineData("N,1,ACME,X,5,10,GTC,AL", "REJECT,2,MALFORMED\n")]
    [InlineData("M,1,ACME,B,10,AL,", "REJECT,2,MALFORMED\n")]
    [InlineData("I,1,ACME,B,5,10,10,AL", "ACCEPT,1\n")]
    [InlineData("I,1,ACME,B,5,10,11,AL", "REJECT,2,MALFORMED\n")]
    [InlineData("A,1,5,0", "REJECT,2,MALFORMED\n")]
    [InlineData("C,1,", "REJECT,2,MALFORMED\n")]
    [InlineData("N,1,ACME,B,5,10,GTC,AL,X,Y,Z", "REJECT,2,MALFORMED\n")]
    [InlineData("NN,1", "REJECT,2,UNKNOWN_COMMAND\n")]
    public void FieldRules(string line, string expected) =>
        Assert.Equal("SYMBOL,ACME\n" + expected, Sessions.Run(Sessions.Lines("S,ACME,1,1", line)));

    [Fact]
    public void BytesOutsideAsciiFailTheNameRule() =>
        Assert.Equal("REJECT,1,MALFORMED\n", System.Text.Encoding.Latin1.GetString(Sessions.Run([(byte)'Q', (byte)',', 0xC9, (byte)'\n'])));
}
