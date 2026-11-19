// The engine of all symbols: it runs one input line at a time and writes the events it produces.
using System.Buffers;

namespace Matching;

public sealed class MatchingEngine(IBufferWriter<byte> output)
{
    private readonly EventWriter events = new(output);
    private readonly Dictionary<string, Book> books = [];
    private readonly Dictionary<long, Order> live = [];
    private readonly HashSet<long> usedIds = [];

    /// <summary>Runs one input line, without its LF and trailing CR, and writes its events.</summary>
    public void Execute(ReadOnlySpan<byte> line, long lineNumber)
    {
        var command = CommandParser.Parse(line, out var reject);
        RejectCode? stateReject = command switch
        {
            null => reject,
            DefineSymbol c => Define(c),
            NewOrder c => Enter(c),
            CancelOrder c => Cancel(c),
            AmendOrder c => Amend(c),
            QueryTop c => Top(c),
            QueryDepth c => Depth(c),
            _ => throw new InvalidOperationException($"unhandled command {command}"),
        };
        if (stateReject is { } code)
        {
            events.Reject(lineNumber, code);
        }
    }

    private RejectCode? Define(DefineSymbol c)
    {
        if (!books.TryAdd(c.Symbol, new Book(c.Symbol, c.Tick, c.Lot)))
        {
            return RejectCode.DuplicateSymbol;
        }

        events.Symbol(c.Symbol);
        return null;
    }

    private RejectCode? Enter(NewOrder c)
    {
        if (usedIds.Contains(c.Id))
        {
            return RejectCode.DuplicateOrderId;
        }

        if (!books.TryGetValue(c.Symbol, out var book))
        {
            return RejectCode.UnknownSymbol;
        }

        if (c.Price is { } limit && limit % book.Tick != 0)
        {
            return RejectCode.InvalidPrice;
        }

        if (c.Qty % book.Lot != 0 || (c.Peak is { } peak && peak % book.Lot != 0))
        {
            return RejectCode.InvalidQty;
        }

        if (c.Tif == TimeInForce.PostOnly && book.WouldTrade(c.Side, c.Price!.Value))
        {
            return RejectCode.WouldTrade;
        }

        usedIds.Add(c.Id);
        events.Accept(c.Id);

        if (c.Tif == TimeInForce.FillOrKill && !book.CanFill(c.Side, c.Price, c.Qty, c.Owner))
        {
            events.Out(c.Id, c.Qty, OutReason.Unfilled);
            return null;
        }

        long open = book.Match(c, live, events);
        if (open == 0)
        {
            return null;
        }

        if (c.Price is { } price && c.Tif is TimeInForce.GoodTillCancel or TimeInForce.PostOnly)
        {
            var order = new Order(c.Id, book, c.Side, c.Owner, c.Peak ?? Order.NoPeak) { Price = price };
            order.Show(open, order.Peak);
            book.SideOf(c.Side).Append(order);
            live.Add(order.Id, order);
        }
        else
        {
            events.Out(c.Id, open, OutReason.Unfilled);
        }

        return null;
    }

    private RejectCode? Cancel(CancelOrder c)
    {
        if (!live.Remove(c.Id, out var order))
        {
            return RejectCode.UnknownOrder;
        }

        order.Book.SideOf(order.Side).Remove(order);
        events.Out(order.Id, order.Open, OutReason.Cancelled);
        return null;
    }

    private RejectCode? Amend(AmendOrder c)
    {
        if (!live.TryGetValue(c.Id, out var order))
        {
            return RejectCode.UnknownOrder;
        }

        var book = order.Book;
        if (c.Price % book.Tick != 0)
        {
            return RejectCode.InvalidPrice;
        }

        if (c.Qty % book.Lot != 0)
        {
            return RejectCode.InvalidQty;
        }

        if (book.WouldTrade(order.Side, c.Price))
        {
            return RejectCode.WouldTrade;
        }

        var side = book.SideOf(order.Side);
        if (c.Price == order.Price && c.Qty <= order.Open)
        {
            side.LevelOf(order).SetDisplayed(order, c.Qty, order.Displayed);
        }
        else
        {
            side.Remove(order);
            order.Price = c.Price;
            order.Show(c.Qty, order.Peak);
            side.Append(order);
        }

        events.Amend(order.Id, c.Price, c.Qty);
        return null;
    }

    private RejectCode? Top(QueryTop c)
    {
        if (!books.TryGetValue(c.Symbol, out var book))
        {
            return RejectCode.UnknownSymbol;
        }

        events.Top(c.Symbol, book.Bids.Best, book.Asks.Best);
        return null;
    }

    private RejectCode? Depth(QueryDepth c)
    {
        if (!books.TryGetValue(c.Symbol, out var book))
        {
            return RejectCode.UnknownSymbol;
        }

        var bids = book.Bids.Levels.Take((int)Math.Min(c.Levels, int.MaxValue)).ToList();
        var asks = book.Asks.Levels.Take((int)Math.Min(c.Levels, int.MaxValue)).ToList();
        events.Depth(c.Symbol, bids.Count, asks.Count);
        bids.ForEach(level => events.Level(Side.Buy, level));
        asks.ForEach(level => events.Level(Side.Sell, level));
        return null;
    }
}
