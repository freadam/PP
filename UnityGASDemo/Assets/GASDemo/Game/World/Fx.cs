using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Rendering;

namespace GASDemo
{
    /// <summary>
    /// Programmer-art toolkit: primitives, flat telegraph meshes and two shared materials tinted per renderer
    /// with MaterialPropertyBlocks. Works in the built-in pipeline and URP without any assets.
    /// </summary>
    public static class Fx
    {
        static readonly int ColorId = Shader.PropertyToID("_Color");
        static readonly int BaseColorId = Shader.PropertyToID("_BaseColor");
        static readonly Dictionary<string, Mesh> MeshCache = new Dictionary<string, Mesh>();
        static Material opaque, transparent;
        static MaterialPropertyBlock block;

        /// <summary>Everything spawned for the demo is parented here so a restart can wipe it in one Destroy.</summary>
        public static Transform Root;

        public static Material Opaque
        {
            get
            {
                if (opaque == null)
                {
                    // Shader.Find returns a real null when missing, but stay explicit with Unity objects.
                    bool srp = GraphicsSettings.currentRenderPipeline != null;
                    var shader = srp ? Shader.Find("Universal Render Pipeline/Lit") : Shader.Find("Standard");
                    if (shader == null && srp) shader = Shader.Find("Universal Render Pipeline/Simple Lit");
                    if (shader == null) shader = Shader.Find("Sprites/Default");
                    opaque = new Material(shader) { name = "GASDemo_Opaque" };
                }
                return opaque;
            }
        }

        /// <summary>Unlit alpha-blended, double sided: telegraphs, bursts, auras.</summary>
        public static Material Transparent
        {
            get
            {
                if (transparent == null)
                {
                    var shader = Shader.Find("GASDemo/UnlitTransparent");
                    if (shader == null) shader = Shader.Find("Sprites/Default");
                    transparent = new Material(shader) { name = "GASDemo_Transparent" };
                }
                return transparent;
            }
        }

        public static void SetColor(Renderer r, Color c)
        {
            if (r == null) return;
            block ??= new MaterialPropertyBlock();
            r.GetPropertyBlock(block);
            block.SetColor(ColorId, c);
            block.SetColor(BaseColorId, c);
            r.SetPropertyBlock(block);
        }

        public static void SetColor(Renderer[] renderers, Color c)
        {
            foreach (var r in renderers) SetColor(r, c);
        }

        public static GameObject Primitive(PrimitiveType type, Vector3 position, Vector3 scale, Color color, bool transparentMat = false, Transform parent = null)
        {
            var go = GameObject.CreatePrimitive(type);
            var col = go.GetComponent<Collider>();
            if (col != null) Object.Destroy(col);
            go.transform.SetParent(parent != null ? parent : Root, false);
            go.transform.position = position;
            go.transform.localScale = scale;
            var r = go.GetComponent<Renderer>();
            r.sharedMaterial = transparentMat ? Transparent : Opaque;
            r.shadowCastingMode = transparentMat ? ShadowCastingMode.Off : ShadowCastingMode.On;
            SetColor(r, color);
            return go;
        }

        /// <summary>A flat mesh object lying on the ground (y = 0.03), for telegraphs and zones.</summary>
        public static GameObject GroundMesh(string name, Mesh mesh, Vector3 position, Quaternion rotation, Color color, Transform parent = null)
        {
            var go = new GameObject(name);
            go.transform.SetParent(parent != null ? parent : Root, false);
            go.transform.position = new Vector3(position.x, 0.03f, position.z);
            go.transform.rotation = rotation;
            go.AddComponent<MeshFilter>().sharedMesh = mesh;
            var r = go.AddComponent<MeshRenderer>();
            r.sharedMaterial = Transparent;
            r.shadowCastingMode = ShadowCastingMode.Off;
            SetColor(r, color);
            return go;
        }

        /// <summary>Flat circular sector on XZ, facing +Z. 360° gives a disc.</summary>
        public static Mesh SectorMesh(float radius, float angleDegrees, float innerRadius = 0f)
        {
            string key = $"sector_{radius:0.00}_{angleDegrees:0.0}_{innerRadius:0.00}";
            if (MeshCache.TryGetValue(key, out var cached) && cached != null) return cached;

            int segments = Mathf.Max(6, Mathf.CeilToInt(angleDegrees / 6f));
            var verts = new List<Vector3>();
            var tris = new List<int>();
            float start = -angleDegrees * 0.5f;
            for (int i = 0; i <= segments; i++)
            {
                float a = (start + angleDegrees * i / segments) * Mathf.Deg2Rad;
                var dir = new Vector3(Mathf.Sin(a), 0f, Mathf.Cos(a));
                verts.Add(dir * innerRadius);
                verts.Add(dir * radius);
                if (i == segments) continue;
                int b = i * 2;
                tris.AddRange(new[] { b, b + 1, b + 3, b, b + 3, b + 2 });
            }
            var mesh = new Mesh { name = key };
            mesh.SetVertices(verts);
            mesh.SetTriangles(tris, 0);
            mesh.RecalculateNormals();
            mesh.RecalculateBounds();
            MeshCache[key] = mesh;
            return mesh;
        }

        /// <summary>Flat rectangle on XZ from the origin forward along +Z.</summary>
        public static Mesh LaneMesh(float length, float width)
        {
            string key = $"lane_{length:0.00}_{width:0.00}";
            if (MeshCache.TryGetValue(key, out var cached) && cached != null) return cached;
            float h = width * 0.5f;
            var mesh = new Mesh { name = key };
            mesh.SetVertices(new List<Vector3> { new Vector3(-h, 0, 0), new Vector3(h, 0, 0), new Vector3(-h, 0, length), new Vector3(h, 0, length) });
            mesh.SetTriangles(new List<int> { 0, 2, 1, 1, 2, 3 }, 0);
            mesh.RecalculateNormals();
            mesh.RecalculateBounds();
            MeshCache[key] = mesh;
            return mesh;
        }

        /// <summary>Expanding, fading one-shot burst.</summary>
        public static void Burst(Vector3 position, Color color, float startSize, float endSize, float lifetime, PrimitiveType type = PrimitiveType.Sphere)
        {
            var go = Primitive(type, position, Vector3.one * startSize, color, true);
            var fade = go.AddComponent<FadeAndDestroy>();
            fade.Lifetime = lifetime;
            fade.StartScale = Vector3.one * startSize;
            fade.EndScale = Vector3.one * endSize;
            fade.BaseColor = color;
        }

        /// <summary>Flat expanding ring on the ground.</summary>
        public static void GroundRing(Vector3 position, Color color, float endRadius, float lifetime)
        {
            var go = GroundMesh("Ring", SectorMesh(1f, 360f, 0.8f), position, Quaternion.identity, color);
            var fade = go.AddComponent<FadeAndDestroy>();
            fade.Lifetime = lifetime;
            fade.StartScale = Vector3.one * 0.2f;
            fade.EndScale = Vector3.one * endRadius;
            fade.BaseColor = color;
        }

        public static Color WithAlpha(this Color c, float a) => new Color(c.r, c.g, c.b, a);
    }

    /// <summary>Scales from StartScale to EndScale while fading alpha out, then destroys itself.</summary>
    public class FadeAndDestroy : MonoBehaviour
    {
        public float Lifetime = 0.3f;
        public Vector3 StartScale = Vector3.one;
        public Vector3 EndScale = Vector3.one;
        public Color BaseColor = Color.white;
        float age;
        Renderer cachedRenderer;

        void Awake() => cachedRenderer = GetComponent<Renderer>();

        void Update()
        {
            age += Time.deltaTime;
            float t = Mathf.Clamp01(age / Lifetime);
            transform.localScale = Vector3.Lerp(StartScale, EndScale, 1f - (1f - t) * (1f - t));
            Fx.SetColor(cachedRenderer, BaseColor.WithAlpha(BaseColor.a * (1f - t)));
            if (age >= Lifetime) Destroy(gameObject);
        }
    }

    /// <summary>Damage numbers etc. Drawn by the HUD in OnGUI.</summary>
    public static class FloatingText
    {
        public class Entry
        {
            public Vector3 WorldPosition;
            public string Text;
            public Color Color;
            public float Age;
            public float Lifetime = 0.9f;
            public Vector2 Jitter;
        }

        public static readonly List<Entry> Entries = new List<Entry>();

        public static void Spawn(Vector3 worldPosition, string text, Color color)
        {
            Entries.Add(new Entry
            {
                WorldPosition = worldPosition, Text = text, Color = color,
                Jitter = new Vector2(Random.Range(-18f, 18f), Random.Range(-6f, 6f)),
            });
        }

        public static void Tick(float dt)
        {
            for (int i = Entries.Count - 1; i >= 0; i--)
            {
                Entries[i].Age += dt;
                if (Entries[i].Age >= Entries[i].Lifetime) Entries.RemoveAt(i);
            }
        }
    }
}
