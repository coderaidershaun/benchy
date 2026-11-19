// The resting orders of one symbol, and the matching of an incoming order against them.
namespace Matching;

internal sealed class Book(string symbol, long tick, long lot)
{
    private readonly BookSide bids = new(Side.Buy);
    private readonly BookSide asks = new(Side.Sell);

    public string Symbol { get; } = symbol;

    public long Tick { get; } = tick;

    public long Lot { get; } = lot;

    public BookSide Bids => bids;

    public BookSide Asks => asks;

    public BookSide SideOf(Side side) => side == Side.Buy ? bids : asks;

    public BookSide Opposite(Side side) => side == Side.Buy ? asks : bids;

    public bool WouldTrade(Side side, long price)
    {
        var opposite = Opposite(side);
        return opposite.Best is { } best && opposite.IsReachable(best.Price, price);
    }

    public bool CanFill(Side side, long? limit, long qty, string owner)
    {
        var opposite = Opposite(side);
        long available = 0;
        foreach (var level in opposite.Levels.TakeWhile(level => opposite.IsReachable(level.Price, limit)))
        {
            foreach (var order in level.Orders)
            {
                if (order.Owner != owner)
                {
                    available += order.Open;
                    if (available >= qty)
                    {
                        return true;
                    }
                }
            }
        }

        return false;
    }

    /// <summary>Matches an incoming order and returns its open quantity that did not trade.</summary>
    public long Match(NewOrder incoming, IDictionary<long, Order> live, EventWriter events)
    {
        var opposite = Opposite(incoming.Side);
        long open = incoming.Qty;
        while (open > 0 && opposite.Best is { } level && opposite.IsReachable(level.Price, incoming.Price))
        {
            var maker = level.First;
            if (maker.Owner == incoming.Owner)
            {
                events.Out(maker.Id, maker.Open, OutReason.SelfTrade);
                opposite.Remove(maker);
                live.Remove(maker.Id);
                continue;
            }

            long qty = Math.Min(open, maker.Displayed);
            events.Trade(Symbol, level.Price, qty, incoming.Id, maker.Id);
            open -= qty;
            level.Fill(maker, qty);
            if (maker.Displayed > 0)
            {
                continue;
            }

            opposite.Remove(maker);
            if (maker.Hidden > 0)
            {
                maker.Show(maker.Hidden, maker.Peak);
                opposite.Append(maker);
            }
            else
            {
                live.Remove(maker.Id);
            }
        }

        return open;
    }
}
