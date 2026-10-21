using UnityEngine;
#if ENABLE_INPUT_SYSTEM && GASDEMO_INPUT_SYSTEM
using UnityEngine.InputSystem;
#endif

namespace GASDemo
{
    public enum DemoButton { Basic, Secondary, Skill, Dash, Ultimate, Confirm, ToggleDebug, Choice1, Choice2, Choice3 }

    /// <summary>
    /// Thin device layer so the demo runs with either the new Input System (package installed + enabled)
    /// or the legacy Input Manager, with zero project setup.
    /// </summary>
    public static class InputReader
    {
#if ENABLE_INPUT_SYSTEM && GASDEMO_INPUT_SYSTEM
        public static Vector2 Move
        {
            get
            {
                var k = Keyboard.current;
                if (k == null) return Vector2.zero;
                float x = (k.dKey.isPressed || k.rightArrowKey.isPressed ? 1f : 0f) - (k.aKey.isPressed || k.leftArrowKey.isPressed ? 1f : 0f);
                float y = (k.wKey.isPressed || k.upArrowKey.isPressed ? 1f : 0f) - (k.sKey.isPressed || k.downArrowKey.isPressed ? 1f : 0f);
                return Vector2.ClampMagnitude(new Vector2(x, y), 1f);
            }
        }

        public static Vector2 MousePosition => Mouse.current != null ? Mouse.current.position.ReadValue() : Vector2.zero;

        static UnityEngine.InputSystem.Controls.ButtonControl Control(DemoButton b)
        {
            var k = Keyboard.current;
            var m = Mouse.current;
            switch (b)
            {
                case DemoButton.Basic: return m?.leftButton;
                case DemoButton.Secondary: return m?.rightButton;
                case DemoButton.Skill: return k?.qKey;
                case DemoButton.Dash: return k?.spaceKey;
                case DemoButton.Ultimate: return k?.rKey;
                case DemoButton.Confirm: return k?.enterKey;
                case DemoButton.ToggleDebug: return k?.f1Key;
                case DemoButton.Choice1: return k?.digit1Key;
                case DemoButton.Choice2: return k?.digit2Key;
                case DemoButton.Choice3: return k?.digit3Key;
            }
            return null;
        }

        public static bool Down(DemoButton b) => Control(b)?.wasPressedThisFrame ?? false;
        public static bool Held(DemoButton b) => Control(b)?.isPressed ?? false;
        public static bool Up(DemoButton b) => Control(b)?.wasReleasedThisFrame ?? false;
#else
        public static Vector2 Move
        {
            get
            {
                float x = (Input.GetKey(KeyCode.D) || Input.GetKey(KeyCode.RightArrow) ? 1f : 0f) - (Input.GetKey(KeyCode.A) || Input.GetKey(KeyCode.LeftArrow) ? 1f : 0f);
                float y = (Input.GetKey(KeyCode.W) || Input.GetKey(KeyCode.UpArrow) ? 1f : 0f) - (Input.GetKey(KeyCode.S) || Input.GetKey(KeyCode.DownArrow) ? 1f : 0f);
                return Vector2.ClampMagnitude(new Vector2(x, y), 1f);
            }
        }

        public static Vector2 MousePosition => Input.mousePosition;

        static KeyCode Key(DemoButton b)
        {
            switch (b)
            {
                case DemoButton.Basic: return KeyCode.Mouse0;
                case DemoButton.Secondary: return KeyCode.Mouse1;
                case DemoButton.Skill: return KeyCode.Q;
                case DemoButton.Dash: return KeyCode.Space;
                case DemoButton.Ultimate: return KeyCode.R;
                case DemoButton.Confirm: return KeyCode.Return;
                case DemoButton.ToggleDebug: return KeyCode.F1;
                case DemoButton.Choice1: return KeyCode.Alpha1;
                case DemoButton.Choice2: return KeyCode.Alpha2;
                case DemoButton.Choice3: return KeyCode.Alpha3;
            }
            return KeyCode.None;
        }

        public static bool Down(DemoButton b) => Input.GetKeyDown(Key(b));
        public static bool Held(DemoButton b) => Input.GetKey(Key(b));
        public static bool Up(DemoButton b) => Input.GetKeyUp(Key(b));
#endif
    }
}
