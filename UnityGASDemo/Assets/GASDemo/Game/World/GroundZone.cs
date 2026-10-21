using System.Collections.Generic;
using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// A persistent area (Frost Field). Every TickInterval it applies its specs to hostiles standing inside.
    /// Paired with a short, refreshing slow effect this gives "slowed while you stand in it" for free.
    /// </summary>
    public class GroundZone : MonoBehaviour
    {
        public AbilitySystemComponent Instigator;
        public Team Team;
        public float Radius = 3f;
        public float Duration = 4f;
        public float TickInterval = 0.5f;
        public Color Color = new Color(0.4f, 0.8f, 1f, 0.35f);
        public readonly List<GameplayEffectSpec> EffectSpecs = new List<GameplayEffectSpec>();

        float age, tickTimer;
        Renderer zoneRenderer;

        public static GroundZone Spawn(Vector3 center, float radius, Color color)
        {
            var go = Fx.GroundMesh("GroundZone", Fx.SectorMesh(radius, 360f), center, Quaternion.identity, color);
            var zone = go.AddComponent<GroundZone>();
            zone.Radius = radius;
            zone.Color = color;
            return zone;
        }

        void Awake() => zoneRenderer = GetComponent<Renderer>();

        void Update()
        {
            float dt = Time.deltaTime;
            age += dt;
            tickTimer -= dt;
            if (tickTimer <= 0f)
            {
                tickTimer += TickInterval;
                Pulse();
            }

            float pulse = 0.75f + 0.25f * Mathf.Sin(age * 8f);
            float fadeOut = Mathf.Clamp01((Duration - age) / 0.3f);
            Fx.SetColor(zoneRenderer, Color.WithAlpha(Color.a * pulse * fadeOut));

            if (age >= Duration) Destroy(gameObject);
        }

        void Pulse()
        {
            foreach (var target in Targeting.HostilesInRadius(Team, transform.position, Radius))
            {
                foreach (var spec in EffectSpecs)
                {
                    spec.Context.AddHitLocation(target.transform.position + Vector3.up * 0.5f);
                    var source = Instigator != null ? Instigator : target.AbilitySystem;
                    source.ApplyGameplayEffectSpecToTarget(spec, target.AbilitySystem);
                }
            }
        }
    }
}
