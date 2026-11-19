// The command-line program: standard input in, events on standard output.
using Engine;

const string Usage = "usage: Engine < commands > events\nReads engine commands from standard input and writes events to standard output.\n";

if (args is ["--help"])
{
    Console.Out.Write(Usage);
    return 0;
}

if (args.Length > 0)
{
    Console.Error.Write(Usage);
    return 2;
}

using var input = Console.OpenStandardInput();
using var output = Console.OpenStandardOutput();
Session.Run(input, output);
return 0;
