using UnityEngine;

namespace GAS
{
    /// <summary>IAbilitySystemInterface: lets any actor say "this is my ASC" without callers knowing its layout.</summary>
    public interface IAbilitySystemInterface
    {
        AbilitySystemComponent GetAbilitySystemComponent();
    }

    /// <summary>UAbilitySystemBlueprintLibrary / UAbilitySystemGlobals helpers.</summary>
    public static class AbilitySystemGlobals
    {
        public static AbilitySystemComponent GetAbilitySystemComponentFromActor(GameObject actor)
        {
            if (actor == null) return null;
            if (actor.TryGetComponent<IAbilitySystemInterface>(out var asi)) return asi.GetAbilitySystemComponent();
            if (actor.TryGetComponent<AbilitySystemComponent>(out var asc)) return asc;
            return actor.GetComponentInParent<AbilitySystemComponent>();
        }

        /// <summary>The standard way gameplay code tells an actor something happened ("Event.Hit", "Event.Kill").</summary>
        public static int SendGameplayEventToActor(GameObject actor, GameplayTag eventTag, GameplayEventData payload)
        {
            var asc = GetAbilitySystemComponentFromActor(actor);
            return asc != null ? asc.HandleGameplayEvent(eventTag, payload) : 0;
        }
    }
}
