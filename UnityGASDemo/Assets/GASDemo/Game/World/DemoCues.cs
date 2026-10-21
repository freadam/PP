using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>One-shot burst at the cue location (hits, heals).</summary>
    public class BurstCue : GameplayCueNotify
    {
        readonly Color color;
        readonly float size;

        public BurstCue(Color color, float size) { this.color = color; this.size = size; }

        public override void HandleGameplayCue(GameObject target, GameplayCueEvent cueEvent, in GameplayCueParameters p)
        {
            if (cueEvent != GameplayCueEvent.Executed && cueEvent != GameplayCueEvent.OnActive) return;
            var pos = p.HasLocation ? p.Location : target.transform.position + Vector3.up;
            Fx.Burst(pos, color, size * 0.3f, size, 0.22f);
        }
    }

    /// <summary>Big expanding ground ring (ultimate impact).</summary>
    public class ShockwaveCue : GameplayCueNotify
    {
        public float Radius = 5f;

        public override void HandleGameplayCue(GameObject target, GameplayCueEvent cueEvent, in GameplayCueParameters p)
        {
            if (cueEvent != GameplayCueEvent.Executed) return;
            var pos = p.HasLocation ? p.Location : target.transform.position;
            Fx.GroundRing(pos, new Color(1f, 0.85f, 0.3f, 0.9f), Radius, 0.45f);
            Fx.Burst(pos + Vector3.up * 0.5f, new Color(1f, 0.9f, 0.5f, 0.6f), 1f, Radius * 1.6f, 0.3f);
        }
    }

    /// <summary>Persistent status indicator that follows the target while the effect is active (stun stars, slow ring...).</summary>
    public class StatusIndicatorCue : GameplayCueNotifyActor
    {
        public enum Shape { HaloAboveHead, RingAtFeet, Aura }

        readonly Color color;
        readonly Shape shape;

        public StatusIndicatorCue(Color color, Shape shape) { this.color = color; this.shape = shape; }

        protected override GameObject Spawn(GameObject target, in GameplayCueParameters p)
        {
            var character = target.GetComponent<CharacterBase>();
            float height = character != null ? character.HealthBarHeight : 2f;
            float radius = character != null ? character.Radius : 0.5f;
            GameObject go;
            switch (shape)
            {
                case Shape.HaloAboveHead:
                    go = Fx.Primitive(PrimitiveType.Cylinder, Vector3.zero, new Vector3(0.7f, 0.04f, 0.7f), color, true, target.transform);
                    go.transform.localPosition = Vector3.up * (height - 0.2f);
                    go.AddComponent<Spinner>().Speed = 360f;
                    break;
                case Shape.RingAtFeet:
                    go = Fx.GroundMesh("SlowRing", Fx.SectorMesh(radius + 0.35f, 360f, radius + 0.1f), target.transform.position, Quaternion.identity, color, target.transform);
                    go.transform.localPosition = Vector3.up * 0.04f;
                    break;
                default:
                    go = Fx.Primitive(PrimitiveType.Sphere, Vector3.zero, Vector3.one * (radius * 3.2f), color, true, target.transform);
                    go.transform.localPosition = Vector3.up * 1f;
                    go.AddComponent<Pulser>();
                    break;
            }
            go.name = "Cue_" + shape;
            return go;
        }
    }

    public class Spinner : MonoBehaviour
    {
        public float Speed = 180f;
        void Update() => transform.Rotate(0f, Speed * Time.deltaTime, 0f, Space.World);
    }

    public class Pulser : MonoBehaviour
    {
        Vector3 baseScale;
        void Awake() => baseScale = transform.localScale;
        void Update() => transform.localScale = baseScale * (1f + 0.06f * Mathf.Sin(Time.time * 10f));
    }

    public static class DemoCues
    {
        public static void RegisterAll()
        {
            GameplayCueManager.Clear();
            GameplayCueManager.Register(GameTags.Cue_Hit_Physical, new BurstCue(new Color(1f, 0.75f, 0.4f, 0.8f), 1.1f));
            GameplayCueManager.Register(GameTags.Cue_Hit_Magic, new BurstCue(new Color(0.4f, 0.85f, 1f, 0.8f), 1.3f));
            GameplayCueManager.Register(GameTags.Cue_Hit_Burn, new BurstCue(new Color(1f, 0.45f, 0.1f, 0.7f), 0.7f));
            GameplayCueManager.Register(GameTags.Cue_Heal, new BurstCue(new Color(0.3f, 1f, 0.4f, 0.6f), 1.6f));
            GameplayCueManager.Register(GameTags.Cue_Shockwave, new ShockwaveCue { Radius = 5.5f });
            GameplayCueManager.Register(GameTags.Cue_Status_Stun, new StatusIndicatorCue(new Color(1f, 0.9f, 0.2f, 0.9f), StatusIndicatorCue.Shape.HaloAboveHead));
            GameplayCueManager.Register(GameTags.Cue_Status_Slow, new StatusIndicatorCue(new Color(0.4f, 0.8f, 1f, 0.8f), StatusIndicatorCue.Shape.RingAtFeet));
            GameplayCueManager.Register(GameTags.Cue_Status_Burn, new StatusIndicatorCue(new Color(1f, 0.4f, 0.05f, 0.25f), StatusIndicatorCue.Shape.Aura));
            GameplayCueManager.Register(GameTags.Cue_Status_Empowered, new StatusIndicatorCue(new Color(0.75f, 0.35f, 1f, 0.3f), StatusIndicatorCue.Shape.Aura));
        }
    }
}
