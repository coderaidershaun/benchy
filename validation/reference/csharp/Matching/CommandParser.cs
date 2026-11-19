// Turns one line of bytes into a command, or into the code of the first line-only rule it breaks.
using System.Text;

namespace Matching;

public static class CommandParser
{
    public const int MaxLineLength = 256;

    private const long MaxId = 999_999_999_999_999_999;
    private const long MaxValue = 1_000_000_000;
    private const int MaxDigits = 18;
    private const int MaxNameLength = 8;

    // One more slot than the longest command, so a line with too many fields never fits exactly.
    private const int MaxFields = 9;

    public static Command? Parse(ReadOnlySpan<byte> line, out RejectCode reject)
    {
        reject = RejectCode.Malformed;
        if (line.Length > MaxLineLength)
        {
            return null;
        }

        Span<Range> ranges = stackalloc Range[MaxFields];
        int count = Split(line, ranges);
        var fields = new Fields(line, ranges[..count]);

        int? expected = fields.Length > 0 && line[ranges[0]].Length == 1 ? FieldCount(line[ranges[0]][0]) : null;
        if (expected is null)
        {
            reject = RejectCode.UnknownCommand;
            return null;
        }

        return count == expected ? ParseFields(line[ranges[0]][0], fields) : null;
    }

    // Like string splitting, except that the last range takes the rest of the line when there are more fields than ranges.
    private static int Split(ReadOnlySpan<byte> line, Span<Range> ranges)
    {
        int start = 0;
        int count = 0;
        while (count < ranges.Length - 1 && line[start..].IndexOf((byte)',') is var comma and >= 0)
        {
            ranges[count++] = new Range(start, start + comma);
            start += comma + 1;
        }

        ranges[count++] = new Range(start, line.Length);
        return count;
    }

    private static int? FieldCount(byte code) => code switch
    {
        (byte)'S' => 4,
        (byte)'N' => 8,
        (byte)'M' => 6,
        (byte)'I' => 8,
        (byte)'C' => 2,
        (byte)'A' => 4,
        (byte)'Q' => 2,
        (byte)'D' => 3,
        _ => null,
    };

    private static Command? ParseFields(byte code, Fields f) => code switch
    {
        (byte)'S' => f.Name(1) is { } symbol && f.Number(2, MaxValue) is { } tick && f.Number(3, MaxValue) is { } lot
            ? new DefineSymbol(symbol, tick, lot)
            : null,
        (byte)'N' => f.Number(1, MaxId) is { } id && f.Name(2) is { } symbol && f.Side(3) is { } side
            && f.Number(4, MaxValue) is { } price && f.Number(5, MaxValue) is { } qty && f.Tif(6) is { } tif
            && f.Name(7) is { } owner
            ? new NewOrder(id, symbol, side, price, qty, tif, null, owner)
            : null,
        (byte)'M' => f.Number(1, MaxId) is { } id && f.Name(2) is { } symbol && f.Side(3) is { } side
            && f.Number(4, MaxValue) is { } qty && f.Name(5) is { } owner
            ? new NewOrder(id, symbol, side, null, qty, TimeInForce.ImmediateOrCancel, null, owner)
            : null,
        (byte)'I' => f.Number(1, MaxId) is { } id && f.Name(2) is { } symbol && f.Side(3) is { } side
            && f.Number(4, MaxValue) is { } price && f.Number(5, MaxValue) is { } qty
            && f.Number(6, qty) is { } peak && f.Name(7) is { } owner
            ? new NewOrder(id, symbol, side, price, qty, TimeInForce.GoodTillCancel, peak, owner)
            : null,
        (byte)'C' => f.Number(1, MaxId) is { } id ? new CancelOrder(id) : null,
        (byte)'A' => f.Number(1, MaxId) is { } id && f.Number(2, MaxValue) is { } price && f.Number(3, MaxValue) is { } qty
            ? new AmendOrder(id, price, qty)
            : null,
        (byte)'Q' => f.Name(1) is { } symbol ? new QueryTop(symbol) : null,
        (byte)'D' => f.Name(1) is { } symbol && f.Number(2, MaxValue) is { } levels ? new QueryDepth(symbol, levels) : null,
        _ => throw new InvalidOperationException($"no fields for command code {code}"),
    };

    private readonly ref struct Fields(ReadOnlySpan<byte> line, ReadOnlySpan<Range> ranges)
    {
        private readonly ReadOnlySpan<byte> line = line;
        private readonly ReadOnlySpan<Range> ranges = ranges;

        public int Length => ranges.Length;

        private ReadOnlySpan<byte> this[int index] => line[ranges[index]];

        public long? Number(int index, long max)
        {
            var field = this[index];
            if (field.Length is 0 or > MaxDigits)
            {
                return null;
            }

            long value = 0;
            foreach (byte b in field)
            {
                if (!char.IsAsciiDigit((char)b))
                {
                    return null;
                }

                value = (value * 10) + (b - '0');
            }

            return value >= 1 && value <= max ? value : null;
        }

        public string? Name(int index)
        {
            var field = this[index];
            if (field.Length is 0 or > MaxNameLength)
            {
                return null;
            }

            foreach (byte b in field)
            {
                if (!char.IsAsciiLetterUpper((char)b) && !char.IsAsciiDigit((char)b))
                {
                    return null;
                }
            }

            return Encoding.ASCII.GetString(field);
        }

        public Side? Side(int index) => this[index] switch
        {
            [(byte)'B'] => Matching.Side.Buy,
            [(byte)'S'] => Matching.Side.Sell,
            _ => null,
        };

        public TimeInForce? Tif(int index)
        {
            var field = this[index];
            if (field.SequenceEqual("GTC"u8))
            {
                return TimeInForce.GoodTillCancel;
            }

            if (field.SequenceEqual("IOC"u8))
            {
                return TimeInForce.ImmediateOrCancel;
            }

            if (field.SequenceEqual("FOK"u8))
            {
                return TimeInForce.FillOrKill;
            }

            return field.SequenceEqual("POST"u8) ? TimeInForce.PostOnly : null;
        }
    }
}
