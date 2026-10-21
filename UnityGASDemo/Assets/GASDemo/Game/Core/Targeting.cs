using System.Collections.Generic;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// Every living character registers here; abilities query it instead of physics, so the demo needs no layers,
    /// colliders or rigidbodies. (A real project would swap these for Physics.OverlapSphere + a target-data system.)
    /// </summary>
    public static class Targeting
    {
        static readonly List<CharacterBase> Characters = new List<CharacterBase>();

        public static IReadOnlyList<CharacterBase> All => Characters;

        public static void Register(CharacterBase c) { if (!Characters.Contains(c)) Characters.Add(c); }
        public static void Unregister(CharacterBase c) => Characters.Remove(c);
        public static void Clear() => Characters.Clear();

        public static bool IsHostile(Team a, CharacterBase b) => b != null && b.Team != a && b.IsAlive;

        public static CharacterBase FindNearestHostile(Team team, Vector3 position, float maxRange = float.MaxValue)
        {
            CharacterBase best = null;
            float bestSqr = maxRange * maxRange;
            foreach (var c in Characters)
            {
                if (!IsHostile(team, c)) continue;
                float d = Flat(c.transform.position - position).sqrMagnitude;
                if (d < bestSqr) { bestSqr = d; best = c; }
            }
            return best;
        }

        public static List<CharacterBase> HostilesInRadius(Team team, Vector3 center, float radius)
        {
            var results = new List<CharacterBase>();
            foreach (var c in Characters)
            {
                if (!IsHostile(team, c)) continue;
                float r = radius + c.Radius;
                if (Flat(c.transform.position - center).sqrMagnitude <= r * r) results.Add(c);
            }
            return results;
        }

        public static List<CharacterBase> HostilesInCone(Team team, Vector3 origin, Vector3 direction, float range, float angleDegrees)
        {
            var results = new List<CharacterBase>();
            direction = Flat(direction).normalized;
            foreach (var c in Characters)
            {
                if (!IsHostile(team, c)) continue;
                var to = Flat(c.transform.position - origin);
                float dist = to.magnitude;
                if (dist > range + c.Radius) continue;
                // Generous for targets overlapping the attacker: anything touching counts.
                if (dist <= c.Radius + 0.3f || Vector3.Angle(direction, to) <= angleDegrees * 0.5f) results.Add(c);
            }
            return results;
        }

        public static Vector3 Flat(Vector3 v) => new Vector3(v.x, 0f, v.z);
    }
}
