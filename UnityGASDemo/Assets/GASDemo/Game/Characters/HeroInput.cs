using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// Player controller: WASD moves (camera-relative), the mouse aims, and buttons are forwarded to the ASC as
    /// ability input IDs. It never calls an ability directly — the ASC decides what that input means.
    /// </summary>
    [RequireComponent(typeof(CharacterBase))]
    public class HeroInput : MonoBehaviour
    {
        static readonly (DemoButton button, AbilityInputID id)[] Bindings =
        {
            (DemoButton.Basic, AbilityInputID.Basic),
            (DemoButton.Secondary, AbilityInputID.Secondary),
            (DemoButton.Skill, AbilityInputID.Skill),
            (DemoButton.Dash, AbilityInputID.Dash),
            (DemoButton.Ultimate, AbilityInputID.Ultimate),
        };

        CharacterBase character;
        Camera cam;

        void Awake() => character = GetComponent<CharacterBase>();

        void Update()
        {
            if (!character.IsAlive || Time.timeScale <= 0f)
            {
                character.MoveInput = Vector3.zero;
                return;
            }
            if (cam == null) cam = Camera.main;

            var move = InputReader.Move;
            if (cam != null)
            {
                var fwd = Targeting.Flat(cam.transform.forward).normalized;
                var right = Targeting.Flat(cam.transform.right).normalized;
                character.MoveInput = fwd * move.y + right * move.x;

                var ray = cam.ScreenPointToRay(InputReader.MousePosition);
                if (new Plane(Vector3.up, Vector3.zero).Raycast(ray, out float enter))
                    character.AimPoint = ray.GetPoint(enter);
            }

            var asc = character.AbilitySystem;
            foreach (var (button, id) in Bindings)
            {
                if (InputReader.Down(button)) asc.AbilityLocalInputPressed((int)id);
                else if (InputReader.Up(button)) asc.AbilityLocalInputReleased((int)id);
            }

            // Hold-to-attack: keep re-pressing basic attack while the button is held and the swing has finished.
            if (InputReader.Held(DemoButton.Basic))
            {
                var spec = asc.FindAbilitySpecFromInputID((int)AbilityInputID.Basic);
                if (spec != null && !spec.IsActive) asc.AbilityLocalInputPressed((int)AbilityInputID.Basic);
            }
        }
    }
}
