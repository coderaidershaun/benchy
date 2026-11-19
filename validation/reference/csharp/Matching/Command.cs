// One input line after it has passed every check that needs no engine state.
namespace Matching;

public abstract record Command;

public sealed record DefineSymbol(string Symbol, long Tick, long Lot) : Command;

/// <summary>A limit, market or iceberg order. Price is null for a market order, and Peak is set only for an iceberg.</summary>
public sealed record NewOrder(
    long Id, string Symbol, Side Side, long? Price, long Qty, TimeInForce Tif, long? Peak, string Owner) : Command;

public sealed record CancelOrder(long Id) : Command;

public sealed record AmendOrder(long Id, long Price, long Qty) : Command;

public sealed record QueryTop(string Symbol) : Command;

public sealed record QueryDepth(string Symbol, long Levels) : Command;
