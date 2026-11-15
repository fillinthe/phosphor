// Phosphor game pad for Unity. Put this on a camera (or any object): people at the show fly it from their phones.
// Phosphor sends OSC over UDP: /pad/<player>/up|down|left|right|a|b (1 or 0) and /pad/<player>/stick x y.
// A = boost, B = trigger (the On Trigger event). No plugin needed: this file reads OSC itself.
using System;
using System.Collections.Concurrent;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Threading;
using UnityEngine;
using UnityEngine.Events;

public class PhosphorPad : MonoBehaviour
{
    [Header("Network")]
    [Tooltip("The OSC port Phosphor sends to (Session > Audience play > Send pad to).")]
    public int port = {PORT};
    [Tooltip("Which player moves this object (one pad with turns = player 1).")]
    public int player = 1;

    [Header("Flying")]
    public float speed = 6f;
    [Tooltip("Degrees per second.")]
    public float turnRate = 70f;
    [Tooltip("Speed is multiplied by this while A is held.")]
    public float boost = 2.5f;
    [Tooltip("Seconds to reach full speed (0 = at once).")]
    public float smoothing = 0.15f;
    [Tooltip("Up and down arrows climb and dive instead of moving forward and back.")]
    public bool upDownClimbs = false;

    [Header("Limits")]
    public bool keepInsideBox = false;
    public Vector3 boxCenter = Vector3.zero;
    public Vector3 boxSize = new Vector3(100f, 40f, 100f);

    [Header("B button")]
    public UnityEvent onTrigger;

    private UdpClient udp;
    private Thread thread;
    private volatile bool running;
    private readonly ConcurrentQueue<(string address, float[] values)> inbox = new ConcurrentQueue<(string, float[])>();
    private Vector2 stick;
    private bool boosting;
    private float forward, turn;

    void OnEnable()
    {
        try
        {
            udp = new UdpClient(port);
            running = true;
            thread = new Thread(Listen) { IsBackground = true };
            thread.Start();
        }
        catch (Exception e)
        {
            Debug.LogError("PhosphorPad: can't listen on UDP port " + port + ": " + e.Message);
        }
    }

    void OnDisable()
    {
        running = false;
        if (udp != null) udp.Close();
        udp = null;
    }

    void Listen()
    {
        var from = new IPEndPoint(IPAddress.Any, 0);
        while (running)
        {
            try
            {
                byte[] data = udp.Receive(ref from);
                Read(data, 0, data.Length);
            }
            catch (Exception)
            {
                // The socket closed (the component was switched off) or a bad packet: keep going while running.
            }
        }
    }

    // One OSC packet: a message, or a bundle of messages.
    void Read(byte[] d, int start, int end)
    {
        if (end - start >= 16 && d[start] == (byte)'#')
        {
            int at = start + 16; // "#bundle\0" + a time tag
            while (at + 4 <= end)
            {
                int size = (d[at] << 24) | (d[at + 1] << 16) | (d[at + 2] << 8) | d[at + 3];
                at += 4;
                if (size <= 0 || at + size > end) break;
                Read(d, at, at + size);
                at += size;
            }
            return;
        }
        int p = start;
        string address = Text(d, ref p, end);
        if (address == null || p >= end || d[p] != (byte)',') return;
        string tags = Text(d, ref p, end);
        if (tags == null) return;
        var values = new System.Collections.Generic.List<float>();
        for (int i = 1; i < tags.Length; i++)
        {
            if (p + 4 > end) break;
            if (tags[i] == 'f')
            {
                byte[] b = { d[p + 3], d[p + 2], d[p + 1], d[p] };
                values.Add(BitConverter.ToSingle(b, 0));
                p += 4;
            }
            else if (tags[i] == 'i')
            {
                values.Add((d[p] << 24) | (d[p + 1] << 16) | (d[p + 2] << 8) | d[p + 3]);
                p += 4;
            }
        }
        inbox.Enqueue((address, values.ToArray()));
    }

    // A null-ended string padded to four bytes.
    static string Text(byte[] d, ref int p, int end)
    {
        int z = p;
        while (z < end && d[z] != 0) z++;
        if (z >= end) return null;
        string s = Encoding.ASCII.GetString(d, p, z - p);
        p = (z + 4) & ~3;
        return s;
    }

    void Update()
    {
        string mine = "/pad/" + player + "/";
        while (inbox.TryDequeue(out var m))
        {
            if (!m.address.StartsWith(mine) || m.values.Length == 0) continue;
            string what = m.address.Substring(mine.Length);
            if (what == "stick" && m.values.Length >= 2) stick = new Vector2(m.values[0], m.values[1]);
            else if (what == "a") boosting = m.values[0] > 0.5f;
            else if (what == "b" && m.values[0] > 0.5f) onTrigger.Invoke();
        }
        float k = smoothing <= 0f ? 1f : 1f - Mathf.Exp(-Time.deltaTime / smoothing);
        forward = Mathf.Lerp(forward, stick.y, k);
        turn = Mathf.Lerp(turn, stick.x, k);
        float v = speed * (boosting ? boost : 1f) * Time.deltaTime;
        transform.Rotate(0f, turn * turnRate * Time.deltaTime, 0f, Space.World);
        transform.position += (upDownClimbs ? Vector3.up : transform.forward) * forward * v;
        if (keepInsideBox)
        {
            Vector3 h = boxSize * 0.5f, c = boxCenter, q = transform.position;
            transform.position = new Vector3(Mathf.Clamp(q.x, c.x - h.x, c.x + h.x), Mathf.Clamp(q.y, c.y - h.y, c.y + h.y), Mathf.Clamp(q.z, c.z - h.z, c.z + h.z));
        }
    }

    void OnDrawGizmosSelected()
    {
        if (!keepInsideBox) return;
        Gizmos.color = Color.green;
        Gizmos.DrawWireCube(boxCenter, boxSize);
    }
}
