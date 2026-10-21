using System;
using System.Collections;
using System.Collections.Generic;
using UnityEngine;

namespace GAS
{
    /// <summary>A set of explicit tags (FGameplayTagContainer). Used for authoring: ability tags, granted tags, requirements...</summary>
    [Serializable]
    public class GameplayTagContainer : IEnumerable<GameplayTag>, IGameplayTagSet
    {
        [SerializeField] List<GameplayTag> tags = new List<GameplayTag>();

        public GameplayTagContainer() { }

        public GameplayTagContainer(params GameplayTag[] initial)
        {
            foreach (var t in initial) AddTag(t);
        }

        public static GameplayTagContainer Of(params GameplayTag[] tags) => new GameplayTagContainer(tags);

        public int Count => tags.Count;
        public bool IsEmpty => tags.Count == 0;

        public void AddTag(GameplayTag tag)
        {
            if (tag.IsValid && !tags.Contains(tag)) tags.Add(tag);
        }

        public bool RemoveTag(GameplayTag tag) => tags.Remove(tag);

        public void AppendTags(GameplayTagContainer other)
        {
            if (other == null) return;
            foreach (var t in other.tags) AddTag(t);
        }

        public void Reset() => tags.Clear();

        /// <summary>{A.B.C}.HasTag(A.B) == true</summary>
        public bool HasTag(GameplayTag query)
        {
            foreach (var t in tags)
                if (t.MatchesTag(query)) return true;
            return false;
        }

        public bool HasTagExact(GameplayTag query) => tags.Contains(query);

        public bool HasAny(GameplayTagContainer other)
        {
            if (other == null) return false;
            foreach (var q in other.tags)
                if (HasTag(q)) return true;
            return false;
        }

        public bool HasAll(GameplayTagContainer other)
        {
            if (other == null) return true;
            foreach (var q in other.tags)
                if (!HasTag(q)) return false;
            return true;
        }

        bool IGameplayTagSet.HasMatchingTag(GameplayTag tag) => HasTag(tag);

        public IEnumerator<GameplayTag> GetEnumerator() => tags.GetEnumerator();
        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        public override string ToString() => tags.Count == 0 ? "{}" : "{" + string.Join(", ", tags) + "}";
    }
}
