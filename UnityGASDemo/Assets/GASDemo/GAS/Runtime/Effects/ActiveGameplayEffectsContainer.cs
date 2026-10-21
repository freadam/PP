using System;
using System.Collections.Generic;
using UnityEngine;

namespace GAS
{
    /// <summary>
    /// FActiveGameplayEffectsContainer: applies specs, ticks durations/periods, handles stacking, and aggregates
    /// active modifiers into attribute CurrentValues. Owned by exactly one <see cref="AbilitySystemComponent"/>.
    /// </summary>
    public class ActiveGameplayEffectsContainer
    {
        readonly AbilitySystemComponent owner;
        readonly List<ActiveGameplayEffect> effects = new List<ActiveGameplayEffect>();
        int nextHandleId = 1;

        public event Action<ActiveGameplayEffect> OnEffectAdded;
        public event Action<ActiveGameplayEffect> OnEffectRemoved;
        public event Action<ActiveGameplayEffect> OnEffectStackChanged;

        public ActiveGameplayEffectsContainer(AbilitySystemComponent owner) { this.owner = owner; }

        public IReadOnlyList<ActiveGameplayEffect> Effects => effects;

        internal ActiveGameplayEffectHandle Apply(GameplayEffectSpec spec)
        {
            var def = spec.Def;
            if (def == null) return ActiveGameplayEffectHandle.Invalid;
            if (!def.ApplicationTagRequirements.RequirementsMet(owner.OwnedTags))
                return ActiveGameplayEffectHandle.Invalid;

            if (!def.RemoveGameplayEffectsWithTags.IsEmpty)
                RemoveActiveEffectsWithTags(def.RemoveGameplayEffectsWithTags);

            if (def.DurationPolicy == EffectDurationPolicy.Instant)
            {
                ExecuteSpec(spec, 1);
                return ActiveGameplayEffectHandle.InstantApplied;
            }

            if (def.StackingType != EffectStackingType.None)
            {
                var existing = FindStackableEffect(spec);
                if (existing != null)
                {
                    if (existing.StackCount < Mathf.Max(1, def.StackLimitCount))
                        existing.StackCount++;
                    if (def.StackDurationRefreshOnApplication && def.DurationPolicy == EffectDurationPolicy.HasDuration)
                    {
                        existing.Duration = spec.CalculateDuration(owner);
                        existing.TimeRemaining = existing.Duration;
                    }
                    RecalculateAttributesFor(existing);
                    OnEffectStackChanged?.Invoke(existing);
                    return existing.Handle;
                }
            }

            var active = new ActiveGameplayEffect
            {
                Handle = new ActiveGameplayEffectHandle(nextHandleId++),
                Spec = spec,
                PeriodTimeRemaining = def.Period,
                GrantedTagsSnapshot = spec.GetAllGrantedTags(),
                EvaluatedMagnitudes = new float[def.Modifiers.Count],
            };
            active.Duration = spec.CalculateDuration(owner);
            active.TimeRemaining = active.Duration;
            for (int i = 0; i < def.Modifiers.Count; i++)
                active.EvaluatedMagnitudes[i] = def.Modifiers[i].Magnitude.Calculate(spec, owner);

            effects.Add(active);
            owner.OwnedTags.UpdateTagCount(active.GrantedTagsSnapshot, +1);
            if (!def.IsPeriodic)
                RecalculateAttributesFor(active);

            owner.InvokeGameplayCues(spec, GameplayCueEvent.OnActive);
            owner.InvokeGameplayCues(spec, GameplayCueEvent.WhileActive);
            OnEffectAdded?.Invoke(active);

            if (def.IsPeriodic && def.ExecutePeriodicEffectOnApplication && !active.IsPendingRemove)
                ExecuteSpec(spec, active.StackCount);

            return active.Handle;
        }

        ActiveGameplayEffect FindStackableEffect(GameplayEffectSpec spec)
        {
            foreach (var e in effects)
            {
                if (e.IsPendingRemove || e.Def != spec.Def) continue;
                if (spec.Def.StackingType == EffectStackingType.AggregateByTarget) return e;
                if (e.Spec.Context.Instigator == spec.Context.Instigator) return e;
            }
            return null;
        }

        /// <summary>Instant (or one periodic tick of) an effect: modifiers and executions change BaseValues.</summary>
        void ExecuteSpec(GameplayEffectSpec spec, int stackCount)
        {
            var def = spec.Def;
            foreach (var mod in def.Modifiers)
            {
                float magnitude = mod.Magnitude.Calculate(spec, owner);
                if (mod.Op == ModifierOp.Add) magnitude *= stackCount;
                owner.ApplyModToAttributeBase(spec, new GameplayModifierEvaluatedData(mod.Attribute, mod.Op, magnitude));
            }

            foreach (var exec in def.Executions)
            {
                if (exec == null) continue;
                var output = new GameplayEffectCustomExecutionOutput();
                exec.Execute(new GameplayEffectCustomExecutionParameters(spec, owner, stackCount), output);
                foreach (var evaluated in output.OutputModifiers)
                    owner.ApplyModToAttributeBase(spec, evaluated);
            }

            owner.InvokeGameplayCues(spec, GameplayCueEvent.Executed);
        }

        internal void Tick(float deltaTime)
        {
            if (effects.Count == 0) return;

            // Snapshot: executing an effect can apply/remove others (death, cleanses) re-entrantly.
            foreach (var e in effects.ToArray())
            {
                if (e.IsPendingRemove) continue;

                if (e.Def.IsPeriodic)
                {
                    e.PeriodTimeRemaining -= deltaTime;
                    while (e.PeriodTimeRemaining <= 0f && !e.IsPendingRemove)
                    {
                        e.PeriodTimeRemaining += e.Def.Period;
                        ExecuteSpec(e.Spec, e.StackCount);
                    }
                }

                if (!e.IsPendingRemove && e.Def.DurationPolicy == EffectDurationPolicy.HasDuration)
                {
                    e.TimeRemaining -= deltaTime;
                    if (e.TimeRemaining <= 0f)
                        Remove(e, -1);
                }
            }
        }

        /// <summary>Removes the whole effect, or <paramref name="stacksToRemove"/> stacks of it.</summary>
        internal bool Remove(ActiveGameplayEffect e, int stacksToRemove)
        {
            if (e == null || e.IsPendingRemove) return false;

            if (stacksToRemove > 0 && e.StackCount > stacksToRemove)
            {
                e.StackCount -= stacksToRemove;
                RecalculateAttributesFor(e);
                OnEffectStackChanged?.Invoke(e);
                return true;
            }

            e.IsPendingRemove = true;
            effects.Remove(e);
            owner.OwnedTags.UpdateTagCount(e.GrantedTagsSnapshot, -1);
            RecalculateAttributesFor(e);
            owner.InvokeGameplayCues(e.Spec, GameplayCueEvent.Removed);
            OnEffectRemoved?.Invoke(e);
            return true;
        }

        void RecalculateAttributesFor(ActiveGameplayEffect e)
        {
            foreach (var mod in e.Def.Modifiers)
                owner.RecalculateCurrentValue(mod.Attribute);
        }

        /// <summary>The aggregator: (Base + ΣAdd) * (1 + Σ(Mul - 1)), or the latest Override.</summary>
        internal float Aggregate(GameplayAttribute attribute, float baseValue)
        {
            float additive = 0f, multiplierBias = 0f, overrideValue = 0f;
            bool hasOverride = false;

            foreach (var e in effects)
            {
                if (e.IsPendingRemove || e.Def.IsPeriodic) continue;
                var mods = e.Def.Modifiers;
                for (int i = 0; i < mods.Count; i++)
                {
                    if (mods[i].Attribute != attribute) continue;
                    float m = e.EvaluatedMagnitudes[i];
                    switch (mods[i].Op)
                    {
                        case ModifierOp.Add: additive += m * e.StackCount; break;
                        case ModifierOp.Multiply: multiplierBias += (m - 1f) * e.StackCount; break;
                        case ModifierOp.Override: hasOverride = true; overrideValue = m; break;
                    }
                }
            }

            return hasOverride ? overrideValue : (baseValue + additive) * Mathf.Max(0f, 1f + multiplierBias);
        }

        public ActiveGameplayEffect GetActiveEffect(ActiveGameplayEffectHandle handle)
        {
            foreach (var e in effects)
                if (e.Handle.Id == handle.Id) return e;
            return null;
        }

        public void GetActiveEffectsWithAnyTags(GameplayTagContainer query, List<ActiveGameplayEffect> results)
        {
            foreach (var e in effects)
                if (!e.IsPendingRemove && e.MatchesAnyOwnedTag(query)) results.Add(e);
        }

        public int RemoveActiveEffectsWithTags(GameplayTagContainer query)
        {
            var matches = new List<ActiveGameplayEffect>();
            GetActiveEffectsWithAnyTags(query, matches);
            foreach (var e in matches) Remove(e, -1);
            return matches.Count;
        }

        /// <summary>Longest remaining time among matching duration effects — what a cooldown UI shows.</summary>
        public bool GetTimeRemainingAndDuration(GameplayTagContainer query, out float timeRemaining, out float duration)
        {
            timeRemaining = 0f;
            duration = 0f;
            bool found = false;
            foreach (var e in effects)
            {
                if (e.IsPendingRemove || e.Def.DurationPolicy != EffectDurationPolicy.HasDuration || !e.MatchesAnyOwnedTag(query)) continue;
                if (!found || e.TimeRemaining > timeRemaining)
                {
                    timeRemaining = e.TimeRemaining;
                    duration = e.Duration;
                }
                found = true;
            }
            return found;
        }

        /// <summary>Shift the remaining time of matching effects; negative deltas shorten (cooldown refunds).</summary>
        public void ModifyTimeRemaining(GameplayTagContainer query, float delta)
        {
            // Snapshot: executing an effect can apply/remove others (death, cleanses) re-entrantly.
            foreach (var e in effects.ToArray())
            {
                if (e.IsPendingRemove || e.Def.DurationPolicy != EffectDurationPolicy.HasDuration || !e.MatchesAnyOwnedTag(query)) continue;
                e.TimeRemaining = Mathf.Min(e.Duration, e.TimeRemaining + delta);
                if (e.TimeRemaining <= 0f) Remove(e, -1);
            }
        }
    }
}
