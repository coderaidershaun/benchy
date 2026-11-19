// The small closed sets of values that commands and events carry.
namespace Matching;

public enum Side
{
    Buy = 1,
    Sell = 2,
}

public enum TimeInForce
{
    GoodTillCancel = 1,
    ImmediateOrCancel = 2,
    FillOrKill = 3,
    PostOnly = 4,
}

public enum RejectCode
{
    UnknownCommand = 1,
    Malformed,
    UnknownSymbol,
    DuplicateSymbol,
    DuplicateOrderId,
    UnknownOrder,
    InvalidPrice,
    InvalidQty,
    WouldTrade,
}

public enum OutReason
{
    Cancelled = 1,
    Unfilled,
    SelfTrade,
}
