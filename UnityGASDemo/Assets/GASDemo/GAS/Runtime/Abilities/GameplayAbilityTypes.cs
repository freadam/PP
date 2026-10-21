using System;
using UnityEngine;

namespace GAS
{
    /// <summary>FGameplayEventData: payload of a gameplay event ("Event.Hit", "Event.Kill"...).</summary>
    public class GameplayEventData
    {
        public GameplayTag EventTag;
        public GameObject Instigator;
        public GameObject Target;
        public object OptionalObject;
        public float EventMagnitude;
        public Vector3 TargetLocation;
        public bool HasTargetLocation;
        public GameplayEffectContext Context;
    }

    public enum AbilityTriggerSource
    {
        /// <summary>Activate when a matching gameplay event is sent to the owner.</summary>
        GameplayEvent,
        /// <summary>Activate when the owner gains a matching tag.</summary>
        OwnedTagAdded,
        /// <summary>Activate when the owner gains a matching tag, cancel when it is lost.</summary>
        OwnedTagPresent,
    }

    /// <summary>FAbilityTriggerData</summary>
    [Serializable]
    public class AbilityTriggerData
    {
        public GameplayTag TriggerTag;
        public AbilityTriggerSource TriggerSource = AbilityTriggerSource.GameplayEvent;

        public AbilityTriggerData() { }

        public AbilityTriggerData(GameplayTag tag, AbilityTriggerSource source)
        {
            TriggerTag = tag;
            TriggerSource = source;
        }
    }

    /// <summary>FGameplayAbilitySpecHandle</summary>
    public readonly struct GameplayAbilitySpecHandle : IEquatable<GameplayAbilitySpecHandle>
    {
        public readonly int Id;
        public GameplayAbilitySpecHandle(int id) { Id = id; }
        public bool IsValid => Id > 0;
        public bool Equals(GameplayAbilitySpecHandle other) => Id == other.Id;
        public override bool Equals(object obj) => obj is GameplayAbilitySpecHandle o && Equals(o);
        public override int GetHashCode() => Id;
        public override string ToString() => $"AbilityHandle({Id})";
    }

    /// <summary>
    /// FGameplayAbilitySpec: an ability granted to an ASC — which ability, at what level, bound to which input.
    /// Abilities here are "instanced per actor": the spec owns a private copy of the ability asset, so the
    /// ability can keep state in fields between activations.
    /// </summary>
    public class GameplayAbilitySpec
    {
        public GameplayAbilitySpecHandle Handle { get; internal set; }
        /// <summary>The authored asset (class default object, in UE terms).</summary>
        public GameplayAbility Definition { get; internal set; }
        /// <summary>This actor's runtime instance of the ability.</summary>
        public GameplayAbility Ability { get; internal set; }
        public int Level { get; set; } = 1;
        public int InputID { get; set; } = -1;
        public bool InputPressed { get; internal set; }
        public object SourceObject { get; set; }

        public bool IsActive => Ability != null && Ability.IsActive;
    }
}
