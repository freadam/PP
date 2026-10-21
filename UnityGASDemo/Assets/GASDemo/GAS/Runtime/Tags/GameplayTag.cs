using System;
using System.Collections.Generic;
using UnityEngine;

namespace GAS
{
    /// <summary>
    /// Hierarchical, dot-separated identifier ("State.Debuff.Stun"), the GAS equivalent of FGameplayTag.
    /// A tag matches any of its parents: "State.Debuff.Stun".MatchesTag("State.Debuff") is true.
    /// Tags are compared by name, so a tag serialized in an asset and one requested in code are the same tag.
    /// </summary>
    [Serializable]
    public struct GameplayTag : IEquatable<GameplayTag>
    {
        [SerializeField] string name;

        // Parent chains are computed once per distinct tag name and shared.
        static readonly Dictionary<string, GameplayTag[]> SelfAndParentsCache = new Dictionary<string, GameplayTag[]>();

        GameplayTag(string tagName) { name = tagName; }

        public static readonly GameplayTag None = default;

        public string Name => name ?? string.Empty;
        public bool IsValid => !string.IsNullOrEmpty(name);

        /// <summary>Equivalent of FGameplayTag::RequestGameplayTag.</summary>
        public static GameplayTag Request(string tagName)
        {
            if (string.IsNullOrWhiteSpace(tagName))
                return None;
            tagName = tagName.Trim();
            if (tagName.StartsWith(".") || tagName.EndsWith(".") || tagName.Contains(".."))
                throw new ArgumentException($"Malformed gameplay tag '{tagName}'");
            return new GameplayTag(tagName);
        }

        /// <summary>True if this tag is <paramref name="other"/> or one of its children.</summary>
        public bool MatchesTag(GameplayTag other)
        {
            if (!IsValid || !other.IsValid) return false;
            if (name.Length == other.name.Length) return string.Equals(name, other.name, StringComparison.Ordinal);
            return name.Length > other.name.Length
                && name[other.name.Length] == '.'
                && name.StartsWith(other.name, StringComparison.Ordinal);
        }

        public bool MatchesTagExact(GameplayTag other) => IsValid && string.Equals(name, other.name, StringComparison.Ordinal);

        public GameplayTag RequestDirectParent()
        {
            if (!IsValid) return None;
            int dot = name.LastIndexOf('.');
            return dot < 0 ? None : new GameplayTag(name.Substring(0, dot));
        }

        /// <summary>"A.B.C" -> [A.B.C, A.B, A]</summary>
        public IReadOnlyList<GameplayTag> GetSelfAndParents()
        {
            if (!IsValid) return Array.Empty<GameplayTag>();
            if (SelfAndParentsCache.TryGetValue(name, out var cached)) return cached;

            var chain = new List<GameplayTag>();
            for (var t = this; t.IsValid; t = t.RequestDirectParent())
                chain.Add(t);
            var result = chain.ToArray();
            SelfAndParentsCache[name] = result;
            return result;
        }

        public bool Equals(GameplayTag other) => string.Equals(Name, other.Name, StringComparison.Ordinal);
        public override bool Equals(object obj) => obj is GameplayTag other && Equals(other);
        public override int GetHashCode() => StringComparer.Ordinal.GetHashCode(Name);
        public static bool operator ==(GameplayTag a, GameplayTag b) => a.Equals(b);
        public static bool operator !=(GameplayTag a, GameplayTag b) => !a.Equals(b);
        public override string ToString() => IsValid ? name : "<None>";
    }

    /// <summary>Anything that can answer "do you have a tag matching X?" — owned-tag counters and plain containers.</summary>
    public interface IGameplayTagSet
    {
        bool HasMatchingTag(GameplayTag tag);
    }

    public static class GameplayTagSetExtensions
    {
        public static bool HasAnyMatchingTags(this IGameplayTagSet set, GameplayTagContainer tags)
        {
            if (tags == null) return false;
            foreach (var t in tags)
                if (set.HasMatchingTag(t)) return true;
            return false;
        }

        public static bool HasAllMatchingTags(this IGameplayTagSet set, GameplayTagContainer tags)
        {
            if (tags == null) return true;
            foreach (var t in tags)
                if (!set.HasMatchingTag(t)) return false;
            return true;
        }
    }
}
