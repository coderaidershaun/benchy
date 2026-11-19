// Properties that must hold for any session, checked on seeded random sessions.
namespace Engine.Tests;

public class InvariantTests
{
    private const int Seeds = 20;
    private const int CommandsPerSession = 2_000;

    public static TheoryData<int> SeedValues() => new(Enumerable.Range(1, Seeds));

    [Theory]
    [MemberData(nameof(SeedValues))]
    public void BookIsNeverCrossed(int seed)
    {
        foreach (string line in Output(RandomSession(seed, withIcebergsAndAmends: true)))
        {
            if (line.StartsWith("TOP,", StringComparison.Ordinal))
            {
                long[] f = [.. line.Split(',').Skip(2).Select(long.Parse)];
                Assert.True(f[0] == 0 || f[2] == 0 || f[0] < f[2], line);
            }
        }
    }

    [Theory]
    [MemberData(nameof(SeedValues))]
    public void QuantityIsConserved(int seed)
    {
        var (input, lastId) = RandomSession(seed, withIcebergsAndAmends: false);
        var cancelAll = Enumerable.Range(1, lastId).Select(id => $"C,{id}\n");
        var entered = new Dictionary<long, long>();
        foreach (string command in input.Split('\n'))
        {
            string[] f = command.Split(',');
            if (f[0] is "N" or "I")
            {
                entered[long.Parse(f[1])] = long.Parse(f[5]);
            }
            else if (f[0] == "M")
            {
                entered[long.Parse(f[1])] = long.Parse(f[4]);
            }
        }

        var accounted = new Dictionary<long, long>();
        void Add(string id, string qty) => accounted[long.Parse(id)] = accounted.GetValueOrDefault(long.Parse(id)) + long.Parse(qty);
        foreach (string line in Output((input + string.Concat(cancelAll), lastId)))
        {
            string[] f = line.Split(',');
            if (f[0] == "TRADE")
            {
                Add(f[4], f[3]);
                Add(f[5], f[3]);
            }
            else if (f[0] == "OUT")
            {
                Add(f[1], f[2]);
            }
        }

        foreach (long id in accounted.Keys)
        {
            Assert.Equal(entered[id], accounted[id]);
        }
    }

    // Ids rise with time and every order trades at its own price, so within one price the maker ids
    // that trade must never go down, unless an iceberg or an amend moves an order to the back.
    [Theory]
    [MemberData(nameof(SeedValues))]
    public void OrdersAtOnePriceFillFirstInFirstOut(int seed)
    {
        var lastMaker = new Dictionary<string, long>();
        foreach (string line in Output(RandomSession(seed, withIcebergsAndAmends: false)))
        {
            string[] f = line.Split(',');
            if (f[0] == "TRADE")
            {
                long maker = long.Parse(f[5]);
                string price = f[2];
                Assert.True(maker >= lastMaker.GetValueOrDefault(price), line);
                lastMaker[price] = maker;
            }
        }
    }

    [Fact]
    public void SameInputGivesSameBytes()
    {
        string input = RandomSession(7, withIcebergsAndAmends: true).Input;
        Assert.Equal(Sessions.Run(input), Sessions.Run(input));
    }

    private static string[] Output((string Input, int LastId) session) =>
        Sessions.Run(session.Input).Split('\n', StringSplitOptions.RemoveEmptyEntries);

    private static (string Input, int LastId) RandomSession(int seed, bool withIcebergsAndAmends)
    {
        var random = new Random(seed);
        var lines = new List<string> { "S,ACME,1,1" };
        string[] owners = ["AL", "BO", "CY"];
        string[] tifs = ["GTC", "GTC", "IOC", "FOK", "POST"];
        int id = 0;
        for (int i = 0; i < CommandsPerSession; i++)
        {
            string side = random.Next(2) == 0 ? "B" : "S";
            string owner = owners[random.Next(owners.Length)];
            int price = random.Next(95, 106);
            int qty = random.Next(1, 30);
            int kind = random.Next(withIcebergsAndAmends ? 10 : 7);
            lines.Add(kind switch
            {
                < 4 => $"N,{++id},ACME,{side},{price},{qty},{tifs[random.Next(tifs.Length)]},{owner}",
                4 => $"M,{++id},ACME,{side},{qty},{owner}",
                5 => $"C,{random.Next(1, id + 2)}",
                6 => "Q,ACME",
                7 or 8 => $"I,{++id},ACME,{side},{price},{qty},{random.Next(1, qty + 1)},{owner}",
                _ => $"A,{random.Next(1, id + 2)},{price},{qty}",
            });
            lines.Add("Q,ACME");
        }

        return (Sessions.Lines([.. lines]), id);
    }
}
