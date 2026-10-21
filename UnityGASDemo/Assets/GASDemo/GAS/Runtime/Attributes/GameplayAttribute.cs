using System;
using UnityEngine;

namespace GAS
{
    /// <summary>
    /// Identifies an attribute (FGameplayAttribute). Serializable so effects authored as assets can point at one.
    /// Attribute sets declare them as static fields: <c>public static readonly GameplayAttribute Health = new("Health");</c>
    /// </summary>
    [Serializable]
    public struct GameplayAttribute : IEquatable<GameplayAttribute>
    {
        [SerializeField] string name;

        public GameplayAttribute(string attributeName) { name = attributeName; }

        public string Name => name ?? string.Empty;
        public bool IsValid => !string.IsNullOrEmpty(name);

        public bool Equals(GameplayAttribute other) => string.Equals(Name, other.Name, StringComparison.Ordinal);
        public override bool Equals(object obj) => obj is GameplayAttribute other && Equals(other);
        public override int GetHashCode() => StringComparer.Ordinal.GetHashCode(Name);
        public static bool operator ==(GameplayAttribute a, GameplayAttribute b) => a.Equals(b);
        public static bool operator !=(GameplayAttribute a, GameplayAttribute b) => !a.Equals(b);
        public override string ToString() => Name;
    }

    /// <summary>
    /// FGameplayAttributeData. BaseValue is permanent (changed by instant/periodic effects);
    /// CurrentValue is BaseValue plus every active duration/infinite modifier, rebuilt by the aggregator.
    /// </summary>
    public class GameplayAttributeData
    {
        public float BaseValue { get; internal set; }
        public float CurrentValue { get; internal set; }

        public GameplayAttributeData(float value)
        {
            BaseValue = value;
            CurrentValue = value;
        }
    }
}
