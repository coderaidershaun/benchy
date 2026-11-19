// Matching rules, each checked on a session written as in the spec: "> " lines are input, the rest is the expected output.
namespace Engine.Tests;

public class BookTests
{
    private static void Check(string transcript)
    {
        var lines = transcript.Split('\n', StringSplitOptions.RemoveEmptyEntries);
        string input = Sessions.Lines([.. lines.Where(l => l.StartsWith("> ", StringComparison.Ordinal)).Select(l => l[2..])]);
        string expected = Sessions.Lines([.. lines.Where(l => !l.StartsWith("> ", StringComparison.Ordinal))]);
        Assert.Equal(expected, Sessions.Run(input));
    }

    [Fact]
    public void SelfTradeCancelsRestingOrdersAndMatchingGoesOn() => Check("""
        > S,ACME,1,1
        SYMBOL,ACME
        > N,1,ACME,S,100,10,GTC,AL
        ACCEPT,1
        > N,2,ACME,S,100,10,GTC,BO
        ACCEPT,2
        > N,3,ACME,S,101,10,GTC,AL
        ACCEPT,3
        > N,4,ACME,B,101,15,GTC,AL
        ACCEPT,4
        OUT,1,10,SELF_TRADE
        TRADE,ACME,100,10,4,2
        OUT,3,10,SELF_TRADE
        > Q,ACME
        TOP,ACME,101,5,0,0
        """);

    [Fact]
    public void FillOrKillIgnoresOwnOrdersAndRemovesOnlyThoseItMeets() => Check("""
        > S,ACME,1,1
        SYMBOL,ACME
        > N,1,ACME,S,100,10,GTC,AL
        ACCEPT,1
        > N,2,ACME,S,100,10,GTC,BO
        ACCEPT,2
        > N,3,ACME,S,100,10,GTC,AL
        ACCEPT,3
        > N,4,ACME,B,100,15,FOK,AL
        ACCEPT,4
        OUT,4,15,UNFILLED
        > N,5,ACME,B,100,10,FOK,AL
        ACCEPT,5
        OUT,1,10,SELF_TRADE
        TRADE,ACME,100,10,5,2
        > Q,ACME
        TOP,ACME,0,0,100,10
        """);

    [Fact]
    public void FillOrKillCountsHiddenQuantity() => Check("""
        > S,ACME,1,1
        SYMBOL,ACME
        > I,1,ACME,S,100,30,10,AL
        ACCEPT,1
        > N,2,ACME,B,100,30,FOK,BO
        ACCEPT,2
        TRADE,ACME,100,10,2,1
        TRADE,ACME,100,10,2,1
        TRADE,ACME,100,10,2,1
        > C,1
        REJECT,4,UNKNOWN_ORDER
        """);

    [Fact]
    public void IcebergRefillsToTheBackOfItsQueueInTheMiddleOfMatching() => Check("""
        > S,ACME,1,1
        SYMBOL,ACME
        > I,1,ACME,S,100,30,10,AL
        ACCEPT,1
        > N,2,ACME,S,100,5,GTC,BO
        ACCEPT,2
        > Q,ACME
        TOP,ACME,0,0,100,15
        > N,3,ACME,B,100,22,GTC,CY
        ACCEPT,3
        TRADE,ACME,100,10,3,1
        TRADE,ACME,100,5,3,2
        TRADE,ACME,100,7,3,1
        > D,ACME,5
        DEPTH,ACME,0,1
        LEVEL,S,100,3,1
        > C,1
        OUT,1,13,CANCELLED
        """);

    [Fact]
    public void IcebergMovesBackEvenWhenTheIncomingOrderIsFilled() => Check("""
        > S,ACME,1,1
        SYMBOL,ACME
        > I,1,ACME,S,100,20,10,AL
        ACCEPT,1
        > N,2,ACME,S,100,5,GTC,BO
        ACCEPT,2
        > N,3,ACME,B,100,10,GTC,CY
        ACCEPT,3
        TRADE,ACME,100,10,3,1
        > N,4,ACME,B,100,5,GTC,DI
        ACCEPT,4
        TRADE,ACME,100,5,4,2
        > Q,ACME
        TOP,ACME,0,0,100,10
        """);

    [Fact]
    public void IncomingIcebergTradesItsWholeQuantityThenRestsShowingItsPeak() => Check("""
        > S,ACME,1,1
        SYMBOL,ACME
        > N,1,ACME,S,100,10,GTC,AL
        ACCEPT,1
        > I,2,ACME,B,100,50,10,BO
        ACCEPT,2
        TRADE,ACME,100,10,2,1
        > N,3,ACME,S,100,25,GTC,CY
        ACCEPT,3
        TRADE,ACME,100,10,3,2
        TRADE,ACME,100,10,3,2
        TRADE,ACME,100,5,3,2
        > Q,ACME
        TOP,ACME,100,5,0,0
        """);

    [Fact]
    public void AmendKeepsTheIcebergPeakAndLosesPriorityWhenRaised() => Check("""
        > S,ACME,1,1
        SYMBOL,ACME
        > I,1,ACME,S,100,30,10,AL
        ACCEPT,1
        > N,2,ACME,S,100,5,GTC,BO
        ACCEPT,2
        > A,1,100,25
        AMEND,1,100,25
        > A,1,100,8
        AMEND,1,100,8
        > Q,ACME
        TOP,ACME,0,0,100,13
        > A,1,100,40
        AMEND,1,100,40
        > N,3,ACME,B,100,7,GTC,CY
        ACCEPT,3
        TRADE,ACME,100,5,3,2
        TRADE,ACME,100,2,3,1
        > Q,ACME
        TOP,ACME,0,0,100,8
        """);

    [Fact]
    public void AmendToTheSameValuesKeepsPriority() => Check("""
        > S,ACME,1,1
        SYMBOL,ACME
        > N,1,ACME,B,100,10,GTC,AL
        ACCEPT,1
        > N,2,ACME,B,100,10,GTC,BO
        ACCEPT,2
        > A,1,100,10
        AMEND,1,100,10
        > N,3,ACME,S,100,5,GTC,CY
        ACCEPT,3
        TRADE,ACME,100,5,3,1
        """);

    [Fact]
    public void OrdersThatNeverRestAreNotLiveAndKeepTheirIds() => Check("""
        > S,ACME,1,1
        SYMBOL,ACME
        > M,1,ACME,B,10,AL
        ACCEPT,1
        OUT,1,10,UNFILLED
        > C,1
        REJECT,3,UNKNOWN_ORDER
        > N,1,ACME,B,100,10,GTC,AL
        REJECT,4,DUPLICATE_ORDER_ID
        > N,2,ACME,B,100,10,POST,AL
        ACCEPT,2
        > N,3,ACME,S,100,10,IOC,BO
        ACCEPT,3
        TRADE,ACME,100,10,3,2
        """);

    [Fact]
    public void ChecksRunInTheProtocolOrder() => Check("""
        > S,ACME,5,10
        SYMBOL,ACME
        > N,1,ACME,B,100,10,GTC,AL
        ACCEPT,1
        > N,1,NOPE,B,3,15,GTC,AL
        REJECT,3,DUPLICATE_ORDER_ID
        > N,2,ACME,B,3,15,GTC,AL
        REJECT,4,INVALID_PRICE
        > I,2,ACME,B,100,20,15,AL
        REJECT,5,INVALID_QTY
        > M,2,ACME,S,15,AL
        REJECT,6,INVALID_QTY
        > A,1,101,15
        REJECT,7,INVALID_PRICE
        """);
}
