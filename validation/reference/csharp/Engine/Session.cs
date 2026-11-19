// Splits an input stream into lines as the protocol states and feeds them to one engine.
using System.Buffers;
using Matching;

namespace Engine;

public static class Session
{
    private const int ChunkSize = 1 << 16;

    public static void Run(Stream input, Stream output)
    {
        ArgumentNullException.ThrowIfNull(input);
        ArgumentNullException.ThrowIfNull(output);

        var events = new ArrayBufferWriter<byte>(ChunkSize);
        var engine = new MatchingEngine(events);
        var line = new LineBuffer();
        long lineNumber = 0;
        var chunk = new byte[ChunkSize];

        void Emit()
        {
            engine.Execute(line.Content, ++lineNumber);
            line.Clear();
            if (events.WrittenCount >= ChunkSize)
            {
                output.Write(events.WrittenSpan);
                events.ResetWrittenCount();
            }
        }

        int read;
        while ((read = input.Read(chunk)) > 0)
        {
            ReadOnlySpan<byte> rest = chunk.AsSpan(0, read);
            int end;
            while ((end = rest.IndexOf((byte)'\n')) >= 0)
            {
                line.Append(rest[..end]);
                Emit();
                rest = rest[(end + 1)..];
            }

            line.Append(rest);
        }

        if (!line.IsEmpty)
        {
            Emit();
        }

        output.Write(events.WrittenSpan);
        output.Flush();
    }

    /// <summary>Holds one line, but never more bytes than it takes to tell that the line is too long.</summary>
    private sealed class LineBuffer
    {
        private readonly byte[] bytes = new byte[CommandParser.MaxLineLength + 2];
        private int length;
        private bool isTruncated;

        public bool IsEmpty => length == 0 && !isTruncated;

        public ReadOnlySpan<byte> Content =>
            !isTruncated && length > 0 && bytes[length - 1] == '\r' ? bytes.AsSpan(0, length - 1) : bytes.AsSpan(0, length);

        public void Append(ReadOnlySpan<byte> part)
        {
            int room = bytes.Length - length;
            if (part.Length > room)
            {
                isTruncated = true;
            }

            int kept = Math.Min(room, part.Length);
            part[..kept].CopyTo(bytes.AsSpan(length));
            length += kept;
        }

        public void Clear()
        {
            length = 0;
            isTruncated = false;
        }
    }
}
