using System;
using System.Collections.Generic;

namespace GAS
{
    /// <summary>
    /// Reference-counted tag set (FGameplayTagCountContainer). This is how an ASC "owns" tags: several sources
    /// (effects, active abilities, loose tags) can grant the same tag, and it stays until every source removes it.
    /// Adding "A.B.C" also counts "A.B" and "A", so HasMatchingTag(A.B) is a single lookup.
    /// </summary>
    public class GameplayTagCountContainer : IGameplayTagSet
    {
        readonly Dictionary<GameplayTag, int> explicitCounts = new Dictionary<GameplayTag, int>();
        readonly Dictionary<GameplayTag, int> matchingCounts = new Dictionary<GameplayTag, int>();
        readonly Dictionary<GameplayTag, Action<GameplayTag, int>> newOrRemovedEvents = new Dictionary<GameplayTag, Action<GameplayTag, int>>();

        /// <summary>Fired for every tag (including implied parents) whose matching count changed: (tag, newCount).</summary>
        public event Action<GameplayTag, int> OnAnyTagCountChanged;

        public IEnumerable<GameplayTag> ExplicitTags => explicitCounts.Keys;

        public bool HasMatchingTag(GameplayTag tag) => tag.IsValid && matchingCounts.TryGetValue(tag, out var c) && c > 0;
        public bool HasTagExact(GameplayTag tag) => tag.IsValid && explicitCounts.TryGetValue(tag, out var c) && c > 0;
        public int GetTagCount(GameplayTag tag) => matchingCounts.TryGetValue(tag, out var c) ? c : 0;
        public int GetExplicitTagCount(GameplayTag tag) => explicitCounts.TryGetValue(tag, out var c) ? c : 0;

        public void UpdateTagCount(GameplayTagContainer tags, int delta)
        {
            if (tags == null) return;
            foreach (var t in tags) UpdateTagCount(t, delta);
        }

        public void UpdateTagCount(GameplayTag tag, int delta)
        {
            if (!tag.IsValid || delta == 0) return;

            int oldExplicit = GetExplicitTagCount(tag);
            int newExplicit = Math.Max(0, oldExplicit + delta);
            int applied = newExplicit - oldExplicit;
            if (applied == 0) return;

            if (newExplicit == 0) explicitCounts.Remove(tag);
            else explicitCounts[tag] = newExplicit;

            // Collect changes first, fire afterwards, so listeners always observe a consistent container.
            var changed = new List<(GameplayTag tag, int oldCount, int newCount)>();
            foreach (var t in tag.GetSelfAndParents())
            {
                int oldCount = GetTagCount(t);
                int newCount = Math.Max(0, oldCount + applied);
                if (newCount == 0) matchingCounts.Remove(t);
                else matchingCounts[t] = newCount;
                changed.Add((t, oldCount, newCount));
            }

            foreach (var (t, oldCount, newCount) in changed)
            {
                OnAnyTagCountChanged?.Invoke(t, newCount);
                bool transition = (oldCount == 0) != (newCount == 0);
                if (transition && newOrRemovedEvents.TryGetValue(t, out var handler))
                    handler?.Invoke(t, newCount);
            }
        }

        /// <summary>Called when <paramref name="tag"/> (or any child of it) is first added, or fully removed.</summary>
        public void RegisterGameplayTagEvent(GameplayTag tag, Action<GameplayTag, int> handler)
        {
            newOrRemovedEvents.TryGetValue(tag, out var existing);
            newOrRemovedEvents[tag] = existing + handler;
        }

        public void UnregisterGameplayTagEvent(GameplayTag tag, Action<GameplayTag, int> handler)
        {
            if (newOrRemovedEvents.TryGetValue(tag, out var existing))
                newOrRemovedEvents[tag] = existing - handler;
        }
    }
}
