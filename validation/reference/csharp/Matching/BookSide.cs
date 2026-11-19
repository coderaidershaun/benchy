// The price levels of one side of a book, kept sorted so the best price is found without a scan.
namespace Matching;

internal sealed class BookSide(Side side)
{
    private readonly Dictionary<long, Level> levels = [];
    private readonly SortedSet<long> prices = [];

    public Side Side { get; } = side;

    public Level? Best => prices.Count == 0 ? null : levels[Side == Side.Buy ? prices.Max : prices.Min];

    /// <summary>The levels from the best price outward.</summary>
    public IEnumerable<Level> Levels => (Side == Side.Buy ? prices.Reverse() : prices).Select(price => levels[price]);

    public void Append(Order order)
    {
        if (!levels.TryGetValue(order.Price, out var level))
        {
            level = new Level(order.Price);
            levels.Add(order.Price, level);
            prices.Add(order.Price);
        }

        level.Append(order);
    }

    public void Remove(Order order)
    {
        var level = levels[order.Price];
        level.Remove(order);
        if (level.OrderCount == 0)
        {
            levels.Remove(order.Price);
            prices.Remove(order.Price);
        }
    }

    public Level LevelOf(Order order) => levels[order.Price];

    /// <summary>Whether an incoming order on the other side, with this limit, can trade at the given price of this side.</summary>
    public bool IsReachable(long price, long? limit) => limit switch
    {
        null => true,
        { } l => Side == Side.Sell ? price <= l : price >= l,
    };
}
