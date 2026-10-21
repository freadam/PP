using System.Collections.Generic;
using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// A skillshot. The ability builds the outgoing effect specs up front and hands them over (the standard UE
    /// pattern: projectiles carry FGameplayEffectSpecHandles), so the projectile itself knows nothing about damage.
    /// On hit it applies the specs and tells its instigator via a gameplay event.
    /// </summary>
    public class Projectile : MonoBehaviour
    {
        public AbilitySystemComponent Instigator;
        public Team Team;
        public Vector3 Direction;
        public float Speed = 20f;
        public float Range = 12f;
        public float Radius = 0.4f;
        public bool Pierce;
        public Color Color = Color.cyan;
        public readonly List<GameplayEffectSpec> EffectSpecs = new List<GameplayEffectSpec>();
        public GameplayTag HitEventTag;

        readonly HashSet<CharacterBase> alreadyHit = new HashSet<CharacterBase>();
        float travelled;

        public static Projectile Spawn(Vector3 origin, Vector3 direction, Color color, float visualSize)
        {
            var go = Fx.Primitive(PrimitiveType.Sphere, origin, Vector3.one * visualSize, color);
            go.name = "Projectile";
            var p = go.AddComponent<Projectile>();
            p.Direction = Targeting.Flat(direction).normalized;
            p.Color = color;
            return p;
        }

        void Update()
        {
            float step = Speed * Time.deltaTime;
            transform.position += Direction * step;
            travelled += step;

            foreach (var target in Targeting.HostilesInRadius(Team, transform.position, Radius))
            {
                if (alreadyHit.Contains(target)) continue;
                alreadyHit.Add(target);
                OnHit(target);
                if (!Pierce)
                {
                    Destroy(gameObject);
                    return;
                }
            }

            if (travelled >= Range)
            {
                Fx.Burst(transform.position, Color.WithAlpha(0.5f), 0.3f, 0.9f, 0.2f);
                Destroy(gameObject);
            }
        }

        void OnHit(CharacterBase target)
        {
            var targetAsc = target.AbilitySystem;
            bool applied = false;
            foreach (var spec in EffectSpecs)
            {
                spec.Context.AddHitLocation(target.transform.position + Vector3.up);
                spec.Context.EffectCauser = gameObject;
                var source = Instigator != null ? Instigator : targetAsc;
                applied |= source.ApplyGameplayEffectSpecToTarget(spec, targetAsc).WasSuccessfullyApplied;
            }

            if (applied && Instigator != null && HitEventTag.IsValid)
            {
                Instigator.HandleGameplayEvent(HitEventTag, new GameplayEventData
                {
                    Instigator = Instigator.AvatarActor,
                    Target = target.gameObject,
                    TargetLocation = target.transform.position,
                    HasTargetLocation = true,
                });
            }
        }
    }
}
