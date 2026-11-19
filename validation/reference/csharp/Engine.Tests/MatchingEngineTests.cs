// Book logic checked on the core library itself: lines go straight into the matching engine, with no program around it.
using System.Buffers;
using System.Text;
using Matching;

namespace Engine.Tests;

public class MatchingEngineTests
{
    private static string Events(params string[] lines)
    {
        var output = new ArrayBufferWriter<byte>();
        var engine = new MatchingEngine(output);
        long lineNumber = 0;
        foreach (string line in lines)
        {
            engine.Execute(Encoding.ASCII.GetBytes(line), ++lineNumber);
        }

        return Encoding.ASCII.GetString(output.WrittenSpan);
    }

    [Fact]
    public void BetterPricesFillFirstAndEqualPricesFillInArrivalOrder() => Assert.Equal(
        Sessions.Lines(
            "SYMBOL,ACME", "ACCEPT,1", "ACCEPT,2", "ACCEPT,3", "ACCEPT,4",
            "TRADE,ACME,100,10,4,2", "TRADE,ACME,100,10,4,3", "TRADE,ACME,101,5,4,1",
            "TOP,ACME,0,0,101,5"),
        Events(
            "S,ACME,1,1",
            "N,1,ACME,S,101,10,GTC,AL",
            "N,2,ACME,S,100,10,GTC,BO",
            "N,3,ACME,S,100,10,GTC,CY",
            "N,4,ACME,B,101,25,GTC,DI",
            "Q,ACME"));

    [Fact]
    public void ACancelledOrderLeavesTheBookAndCannotBeCancelledAgain() => Assert.Equal(
        Sessions.Lines("SYMBOL,ACME", "ACCEPT,1", "OUT,1,10,CANCELLED", "REJECT,4,UNKNOWN_ORDER", "TOP,ACME,0,0,0,0"),
        Events("S,ACME,1,1", "N,1,ACME,B,99,10,GTC,AL", "C,1", "C,1", "Q,ACME"));

    [Fact]
    public void ARejectedOrderLeavesTheBookAsItWas() => Assert.Equal(
        Sessions.Lines("SYMBOL,ACME", "ACCEPT,1", "REJECT,3,DUPLICATE_ORDER_ID", "TOP,ACME,99,10,0,0"),
        Events("S,ACME,1,1", "N,1,ACME,B,99,10,GTC,AL", "N,1,ACME,B,98,5,GTC,BO", "Q,ACME"));

    [Fact]
    public void PriceAndQuantityMustBeMultiplesOfTheTickAndLot() => Assert.Equal(
        Sessions.Lines("SYMBOL,ACME", "REJECT,2,INVALID_PRICE", "REJECT,3,INVALID_QTY", "TOP,ACME,0,0,0,0"),
        Events("S,ACME,5,10", "N,1,ACME,B,97,10,GTC,AL", "N,2,ACME,B,95,15,GTC,AL", "Q,ACME"));

    [Fact]
    public void ImmediateOrCancelTradesAndPostOnlyRestsOnlyWhenItWouldNotTrade() => Assert.Equal(
        Sessions.Lines(
            "SYMBOL,ACME", "ACCEPT,1", "ACCEPT,2", "TRADE,ACME,100,4,2,1", "ACCEPT,3",
            "REJECT,5,WOULD_TRADE", "TOP,ACME,99,4,100,6"),
        Events(
            "S,ACME,1,1",
            "N,1,ACME,S,100,10,GTC,AL",
            "N,2,ACME,B,101,4,IOC,BO",
            "N,3,ACME,B,99,4,POST,BO",
            "N,4,ACME,B,100,4,POST,BO",
            "Q,ACME"));

    [Fact]
    public void RejectionsCarryTheLineNumberTheCallerGives()
    {
        var output = new ArrayBufferWriter<byte>();
        var engine = new MatchingEngine(output);

        engine.Execute("Q,ACME"u8, 42);

        Assert.Equal("REJECT,42,UNKNOWN_SYMBOL\n", Encoding.ASCII.GetString(output.WrittenSpan));
    }
}
