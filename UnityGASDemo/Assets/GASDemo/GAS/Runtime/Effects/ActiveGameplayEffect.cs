namespace GAS
{
    /// <summary>FActiveGameplayEffect: a duration/infinite effect currently living on an ASC.</summary>
    public class ActiveGameplayEffect
    {
        public ActiveGameplayEffectHandle Handle { get; internal set; }
        public GameplayEffectSpec Spec { get; internal set; }
        public float Duration { get; internal set; }
        public float TimeRemaining { get; internal set; }
        public float PeriodTimeRemaining { get; internal set; }
        public int StackCount { get; internal set; } = 1;
        public bool IsPendingRemove { get; internal set; }

        /// <summary>Modifier magnitudes captured when the effect was applied (snapshot), indexed like Def.Modifiers.</summary>
        internal float[] EvaluatedMagnitudes;

        /// <summary>Tags this effect granted to the owner, kept so removal takes back exactly what was given.</summary>
        internal GameplayTagContainer GrantedTagsSnapshot;

        public GameplayEffect Def => Spec.Def;
        public bool IsInfinite => Def.DurationPolicy == EffectDurationPolicy.Infinite;

        /// <summary>True if any asset or granted tag matches one of <paramref name="query"/>.</summary>
        public bool MatchesAnyOwnedTag(GameplayTagContainer query)
        {
            if (query == null || query.IsEmpty) return false;
            return Spec.GetAllAssetTags().HasAny(query) || GrantedTagsSnapshot.HasAny(query);
        }
    }
}
