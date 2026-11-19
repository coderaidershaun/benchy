// Writes events in the exact byte format of the protocol.
using System.Buffers;
using System.Globalization;
using System.Text;

namespace Matching;

internal sealed class EventWriter(IBufferWriter<byte> output)
{
    private const int MaxNumberLength = 20;

    public void Symbol(string symbol)
    {
        Write("SYMBOL"u8);
        Field(symbol);
        End();
    }

    public void Accept(long id)
    {
        Write("ACCEPT"u8);
        Field(id);
        End();
    }

    public void Reject(long line, RejectCode code)
    {
        Write("REJECT"u8);
        Field(line);
        Comma();
        Write(code switch
        {
            RejectCode.UnknownCommand => "UNKNOWN_COMMAND"u8,
            RejectCode.Malformed => "MALFORMED"u8,
            RejectCode.UnknownSymbol => "UNKNOWN_SYMBOL"u8,
            RejectCode.DuplicateSymbol => "DUPLICATE_SYMBOL"u8,
            RejectCode.DuplicateOrderId => "DUPLICATE_ORDER_ID"u8,
            RejectCode.UnknownOrder => "UNKNOWN_ORDER"u8,
            RejectCode.InvalidPrice => "INVALID_PRICE"u8,
            RejectCode.InvalidQty => "INVALID_QTY"u8,
            RejectCode.WouldTrade => "WOULD_TRADE"u8,
            _ => throw new ArgumentOutOfRangeException(nameof(code), code, null),
        });
        End();
    }

    public void Trade(string symbol, long price, long qty, long taker, long maker)
    {
        Write("TRADE"u8);
        Field(symbol);
        Field(price);
        Field(qty);
        Field(taker);
        Field(maker);
        End();
    }

    public void Out(long id, long qty, OutReason reason)
    {
        Write("OUT"u8);
        Field(id);
        Field(qty);
        Comma();
        Write(reason switch
        {
            OutReason.Cancelled => "CANCELLED"u8,
            OutReason.Unfilled => "UNFILLED"u8,
            OutReason.SelfTrade => "SELF_TRADE"u8,
            _ => throw new ArgumentOutOfRangeException(nameof(reason), reason, null),
        });
        End();
    }

    public void Amend(long id, long price, long qty)
    {
        Write("AMEND"u8);
        Field(id);
        Field(price);
        Field(qty);
        End();
    }

    public void Top(string symbol, Level? bid, Level? ask)
    {
        Write("TOP"u8);
        Field(symbol);
        Field(bid?.Price ?? 0);
        Field(bid?.Displayed ?? 0);
        Field(ask?.Price ?? 0);
        Field(ask?.Displayed ?? 0);
        End();
    }

    public void Depth(string symbol, int bidLevels, int askLevels)
    {
        Write("DEPTH"u8);
        Field(symbol);
        Field(bidLevels);
        Field(askLevels);
        End();
    }

    public void Level(Side side, Level level)
    {
        Write("LEVEL"u8);
        Comma();
        Write(side == Side.Buy ? "B"u8 : "S"u8);
        Field(level.Price);
        Field(level.Displayed);
        Field(level.OrderCount);
        End();
    }

    private void Field(string name)
    {
        Comma();
        int written = Encoding.ASCII.GetBytes(name, output.GetSpan(name.Length));
        output.Advance(written);
    }

    private void Field(long number)
    {
        Comma();
        Span<byte> span = output.GetSpan(MaxNumberLength);
        if (!number.TryFormat(span, out int written, default, CultureInfo.InvariantCulture))
        {
            throw new InvalidOperationException($"no room to format {number}");
        }

        output.Advance(written);
    }

    private void Comma() => Write(","u8);

    private void End() => Write("\n"u8);

    private void Write(ReadOnlySpan<byte> bytes) => output.Write(bytes);
}
