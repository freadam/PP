using System.Collections.Generic;
using UnityEngine;

namespace GAS
{
    /// <summary>EGameplayCueEvent</summary>
    public enum GameplayCueEvent { OnActive, WhileActive, Executed, Removed }

    /// <summary>FGameplayCueParameters</summary>
    public struct GameplayCueParameters
    {
        public AbilitySystemComponent Instigator;
        public GameObject EffectCauser;
        public Vector3 Location;
        public bool HasLocation;
        public float RawMagnitude;
        public object SourceObject;
    }

    /// <summary>
    /// Cosmetic-only reaction to a "GameplayCue.*" tag (UGameplayCueNotify_Static). Gameplay code never knows what a cue
    /// looks like; it only fires the tag. Keep notifies stateless where possible — see
    /// <see cref="GameplayCueNotifyActor"/> for persistent per-target visuals.
    /// </summary>
    public abstract class GameplayCueNotify
    {
        public abstract void HandleGameplayCue(GameObject target, GameplayCueEvent cueEvent, in GameplayCueParameters parameters);
    }

    /// <summary>
    /// AGameplayCueNotify_Actor: spawns something on OnActive and destroys it on Removed, ref-counted per target so
    /// two overlapping stuns show one indicator that disappears only when both end.
    /// </summary>
    public abstract class GameplayCueNotifyActor : GameplayCueNotify
    {
        readonly Dictionary<GameObject, (GameObject instance, int count)> active = new Dictionary<GameObject, (GameObject, int)>();

        protected abstract GameObject Spawn(GameObject target, in GameplayCueParameters parameters);
        protected virtual void Despawn(GameObject instance) { if (instance != null) Object.Destroy(instance); }

        public override void HandleGameplayCue(GameObject target, GameplayCueEvent cueEvent, in GameplayCueParameters parameters)
        {
            if (target == null) return;
            switch (cueEvent)
            {
                case GameplayCueEvent.OnActive:
                    if (active.TryGetValue(target, out var entry) && entry.instance != null)
                        active[target] = (entry.instance, entry.count + 1);
                    else
                        active[target] = (Spawn(target, parameters), 1);
                    break;

                case GameplayCueEvent.Removed:
                    if (!active.TryGetValue(target, out var existing)) break;
                    if (existing.count > 1)
                    {
                        active[target] = (existing.instance, existing.count - 1);
                    }
                    else
                    {
                        Despawn(existing.instance);
                        active.Remove(target);
                    }
                    break;
            }
        }
    }

    /// <summary>
    /// UGameplayCueManager: routes cue tags to notifies. Lookup walks up the hierarchy, so a notify registered for
    /// "GameplayCue.Hit" handles "GameplayCue.Hit.Magic" unless something more specific is registered.
    /// </summary>
    public static class GameplayCueManager
    {
        static readonly Dictionary<GameplayTag, GameplayCueNotify> Notifies = new Dictionary<GameplayTag, GameplayCueNotify>();

        public static void Register(GameplayTag cueTag, GameplayCueNotify notify) => Notifies[cueTag] = notify;

        public static void Clear() => Notifies.Clear();

        public static void HandleGameplayCue(GameObject target, GameplayTag cueTag, GameplayCueEvent cueEvent, in GameplayCueParameters parameters)
        {
            foreach (var t in cueTag.GetSelfAndParents())
            {
                if (Notifies.TryGetValue(t, out var notify))
                {
                    notify.HandleGameplayCue(target, cueEvent, parameters);
                    return;
                }
            }
        }
    }
}
