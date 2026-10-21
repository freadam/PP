using System;
using System.Collections.Generic;
using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// Between-wave upgrades (Ravenswatch talents / Curse of the Dead Gods relics). Each one is just a GAS operation:
    /// apply an infinite effect, grant a passive ability, or raise an ability spec's level.
    /// </summary>
    public class Boon
    {
        public string Title;
        public string Description;
        public Func<CharacterBase, bool> IsAvailable = _ => true;
        public Action<CharacterBase> Apply;

        public static List<Boon> CreatePool(DemoContent content)
        {
            var pool = new List<Boon>
            {
                Stat("Might", "+12 Attack Power", content.GE_Boon_Might),
                Stat("Sorcery", "+30 Ability Power", content.GE_Boon_Sorcery),
                Stat("Quickening", "+25 Ability Haste (shorter cooldowns)", content.GE_Boon_Haste),
                Stat("Titan's Blood", "+150 Max Health", content.GE_Boon_Vitality),
                Stat("Fleet Foot", "+12% Move Speed", content.GE_Boon_Swiftness),
                Stat("Iron Skin", "+20 Armor", content.GE_Boon_Armor),
                GrantPassive("Ignite", "Every hit burns the target. Burn stacks up to 4 times.", content.GA_Ignite),
                GrantPassive("Bloodthirst", "Kills heal you for 6% of max health.", content.GA_Bloodthirst),
                RankUp(content.GA_Basic),
                RankUp(content.GA_MysticShot),
                RankUp(content.GA_FrostField),
                RankUp(content.GA_Tumble),
                RankUp(content.GA_Shockwave),
            };
            return pool;
        }

        static Boon Stat(string title, string description, GameplayEffect effect) => new Boon
        {
            Title = title,
            Description = description,
            Apply = hero => hero.AbilitySystem.ApplyGameplayEffectToSelf(effect),
        };

        static Boon GrantPassive(string title, string description, GameplayAbility ability) => new Boon
        {
            Title = title,
            Description = description,
            IsAvailable = hero => hero.AbilitySystem.FindAbilitySpecFromDefinition(ability) == null,
            Apply = hero => hero.AbilitySystem.GiveAbility(ability),
        };

        static Boon RankUp(DemoGameplayAbility ability) => new Boon
        {
            Title = ability.DisplayName + " +",
            Description = $"Rank up {ability.DisplayName}: more damage, lower cooldown.",
            IsAvailable = hero =>
            {
                var spec = hero.AbilitySystem.FindAbilitySpecFromDefinition(ability);
                return spec != null && spec.Level < ability.MaxLevel;
            },
            Apply = hero =>
            {
                var spec = hero.AbilitySystem.FindAbilitySpecFromDefinition(ability);
                if (spec != null) spec.Level = Mathf.Min(ability.MaxLevel, spec.Level + 1);
            },
        };

        public static List<Boon> Roll(List<Boon> pool, CharacterBase hero, int count)
        {
            var candidates = pool.FindAll(b => b.IsAvailable(hero));
            var picks = new List<Boon>();
            while (picks.Count < count && candidates.Count > 0)
            {
                int i = UnityEngine.Random.Range(0, candidates.Count);
                picks.Add(candidates[i]);
                candidates.RemoveAt(i);
            }
            return picks;
        }
    }
}
