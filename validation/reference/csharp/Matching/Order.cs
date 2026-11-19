// A resting order and the queue of one price level that holds it.
namespace Matching;

internal sealed class Order(long id, Book book, Side side, string owner, long peak)
{
    // A plain order has no peak, so it always shows its whole open quantity.
    public const long NoPeak = long.MaxValue;

    public long Id { get; } = id;

    public Book Book { get; } = book;

    public Side Side { get; } = side;

    public string Owner { get; } = owner;

    public long Peak { get; } = peak;

    public long Price { get; set; }

    public long Displayed { get; set; }

    public long Hidden { get; set; }

    public long Open => Displayed + Hidden;

    public LinkedListNode<Order>? Node { get; set; }

    public void Show(long open, long maxDisplayed)
    {
        Displayed = Math.Min(maxDisplayed, open);
        Hidden = open - Displayed;
    }
}

internal sealed class Level(long price)
{
    private readonly LinkedList<Order> queue = new();

    public long Price { get; } = price;

    public long Displayed { get; private set; }

    public int OrderCount => queue.Count;

    public Order First => queue.First?.Value ?? throw new InvalidOperationException($"level {Price} is empty");

    public IEnumerable<Order> Orders => queue;

    public void Append(Order order)
    {
        order.Node = queue.AddLast(order);
        Displayed += order.Displayed;
    }

    public void Remove(Order order)
    {
        queue.Remove(order.Node ?? throw new InvalidOperationException($"order {order.Id} is not queued"));
        order.Node = null;
        Displayed -= order.Displayed;
    }

    public void Fill(Order order, long qty)
    {
        order.Displayed -= qty;
        Displayed -= qty;
    }

    public void SetDisplayed(Order order, long open, long maxDisplayed)
    {
        Displayed -= order.Displayed;
        order.Show(open, maxDisplayed);
        Displayed += order.Displayed;
    }
}
