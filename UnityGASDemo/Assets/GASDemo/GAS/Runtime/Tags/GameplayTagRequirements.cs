using System;

namespace GAS
{
    /// <summary>FGameplayTagRequirements: the target must have all RequireTags and none of IgnoreTags.</summary>
    [Serializable]
    public class GameplayTagRequirements
    {
        public GameplayTagContainer RequireTags = new GameplayTagContainer();
        public GameplayTagContainer IgnoreTags = new GameplayTagContainer();

        public bool IsEmpty => RequireTags.IsEmpty && IgnoreTags.IsEmpty;

        public bool RequirementsMet(IGameplayTagSet tags)
        {
            return tags.HasAllMatchingTags(RequireTags) && !tags.HasAnyMatchingTags(IgnoreTags);
        }
    }
}
