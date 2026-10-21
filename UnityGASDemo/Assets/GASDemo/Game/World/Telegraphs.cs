using UnityEngine;

namespace GASDemo
{
    /// <summary>Danger-zone decals: a faint outline plus an inner fill that grows until the hit lands.</summary>
    public static class Telegraphs
    {
        public static GameObject Create(Vector3 position, Quaternion rotation, float radius, float angle, float fillDuration, Color color)
        {
            var root = Fx.GroundMesh("Telegraph", Fx.SectorMesh(radius, angle), position, rotation, color.WithAlpha(color.a * 0.6f));
            var fill = Fx.GroundMesh("Fill", Fx.SectorMesh(radius, angle), position, rotation, color.WithAlpha(Mathf.Min(1f, color.a * 1.6f)), root.transform);
            fill.transform.localPosition = Vector3.up * 0.005f;
            fill.AddComponent<TelegraphFill>().Duration = fillDuration;
            return root;
        }
    }

    public class TelegraphFill : MonoBehaviour
    {
        public float Duration = 0.5f;
        float age;

        void Awake() => transform.localScale = Vector3.zero;

        void Update()
        {
            age += Time.deltaTime;
            transform.localScale = Vector3.one * Mathf.Clamp01(age / Mathf.Max(0.01f, Duration));
        }
    }
}
