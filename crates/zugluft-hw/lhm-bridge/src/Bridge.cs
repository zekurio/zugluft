namespace zugluft_lhm_bridge;

using System;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using LibreHardwareMonitor.Hardware;

[StructLayout(LayoutKind.Sequential)]
public readonly struct ComputerResult
{
    public readonly IntPtr Computer;
    public readonly IntPtr Error;

    private ComputerResult(IntPtr computer, IntPtr error)
    {
        Computer = computer;
        Error = error;
    }

    public static ComputerResult Success(Computer computer) =>
        new(ToHandle(computer), IntPtr.Zero);

    public static ComputerResult Failure(Exception error) =>
        new(IntPtr.Zero, Strings.Alloc(error.ToString()));

    private static IntPtr ToHandle(object value) =>
        GCHandle.ToIntPtr(GCHandle.Alloc(value));
}

[StructLayout(LayoutKind.Sequential)]
public readonly struct PtrArray
{
    public readonly UIntPtr Length;
    public readonly IntPtr Data;
    public readonly IntPtr Handle;

    public PtrArray(IntPtr[] values)
    {
        Length = (UIntPtr)values.Length;
        if (values.Length == 0)
        {
            Data = IntPtr.Zero;
            Handle = IntPtr.Zero;
            return;
        }

        GCHandle handle = GCHandle.Alloc(values, GCHandleType.Pinned);
        Data = handle.AddrOfPinnedObject();
        Handle = GCHandle.ToIntPtr(handle);
    }

    public void Free()
    {
        if (Handle != IntPtr.Zero)
        {
            GCHandle.FromIntPtr(Handle).Free();
        }
    }
}

public static class Bridge
{
    [UnmanagedCallersOnly(EntryPoint = "free_string", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static void FreeString(IntPtr ptr) => Strings.Free(ptr);

    [UnmanagedCallersOnly(EntryPoint = "free_ptr_array", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static void FreePtrArray(PtrArray array) => array.Free();

    [UnmanagedCallersOnly(EntryPoint = "create_computer", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static ComputerResult CreateComputer()
    {
        try
        {
            Computer computer = new()
            {
                IsBatteryEnabled = true,
                IsControllerEnabled = true,
                IsCpuEnabled = true,
                IsGpuEnabled = true,
                IsMemoryEnabled = true,
                IsMotherboardEnabled = true,
                IsNetworkEnabled = false,
                IsPsuEnabled = true,
                IsStorageEnabled = true,
            };
            computer.Open();
            return ComputerResult.Success(computer);
        }
        catch (Exception error)
        {
            return ComputerResult.Failure(error);
        }
    }

    [UnmanagedCallersOnly(EntryPoint = "update_computer", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static void UpdateComputer(IntPtr ptr)
    {
        try
        {
            if (Target<Computer>(ptr) is { } computer)
            {
                computer.Accept(new UpdateVisitor());
            }
        }
        catch
        {
        }
    }

    [UnmanagedCallersOnly(EntryPoint = "free_computer", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static void FreeComputer(IntPtr ptr)
    {
        try
        {
            if (Target<Computer>(ptr) is { } computer)
            {
                computer.Close();
            }
            FreeHandle(ptr);
        }
        catch
        {
        }
    }

    [UnmanagedCallersOnly(EntryPoint = "get_computer_hardware", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static PtrArray GetComputerHardware(IntPtr ptr)
    {
        try
        {
            return Target<Computer>(ptr) is { } computer
                ? ToPtrArray(computer.Hardware.Cast<object>())
                : new PtrArray(Array.Empty<IntPtr>());
        }
        catch
        {
            return new PtrArray(Array.Empty<IntPtr>());
        }
    }

    [UnmanagedCallersOnly(EntryPoint = "get_hardware_children", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static PtrArray GetHardwareChildren(IntPtr ptr)
    {
        try
        {
            return Target<IHardware>(ptr) is { } hardware
                ? ToPtrArray(hardware.SubHardware.Cast<object>())
                : new PtrArray(Array.Empty<IntPtr>());
        }
        catch
        {
            return new PtrArray(Array.Empty<IntPtr>());
        }
    }

    [UnmanagedCallersOnly(EntryPoint = "get_hardware_sensors", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static PtrArray GetHardwareSensors(IntPtr ptr)
    {
        try
        {
            return Target<IHardware>(ptr) is { } hardware
                ? ToPtrArray(hardware.Sensors.Cast<object>())
                : new PtrArray(Array.Empty<IntPtr>());
        }
        catch
        {
            return new PtrArray(Array.Empty<IntPtr>());
        }
    }

    [UnmanagedCallersOnly(EntryPoint = "get_hardware_identifier", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static IntPtr GetHardwareIdentifier(IntPtr ptr)
    {
        try { return Strings.Alloc(Target<IHardware>(ptr)?.Identifier.ToString()); }
        catch { return IntPtr.Zero; }
    }

    [UnmanagedCallersOnly(EntryPoint = "get_hardware_name", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static IntPtr GetHardwareName(IntPtr ptr)
    {
        try { return Strings.Alloc(Target<IHardware>(ptr)?.Name); }
        catch { return IntPtr.Zero; }
    }

    [UnmanagedCallersOnly(EntryPoint = "get_hardware_type", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static int GetHardwareType(IntPtr ptr)
    {
        try { return Target<IHardware>(ptr) is { } hardware ? (int)hardware.HardwareType : -1; }
        catch { return -1; }
    }

    [UnmanagedCallersOnly(EntryPoint = "free_hardware", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static void FreeHardware(IntPtr ptr) => FreeHandle(ptr);

    [UnmanagedCallersOnly(EntryPoint = "get_sensor_identifier", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static IntPtr GetSensorIdentifier(IntPtr ptr)
    {
        try { return Strings.Alloc(Target<ISensor>(ptr)?.Identifier.ToString()); }
        catch { return IntPtr.Zero; }
    }

    [UnmanagedCallersOnly(EntryPoint = "get_sensor_name", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static IntPtr GetSensorName(IntPtr ptr)
    {
        try { return Strings.Alloc(Target<ISensor>(ptr)?.Name); }
        catch { return IntPtr.Zero; }
    }

    [UnmanagedCallersOnly(EntryPoint = "get_sensor_type", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static int GetSensorType(IntPtr ptr)
    {
        try { return Target<ISensor>(ptr) is { } sensor ? (int)sensor.SensorType : -1; }
        catch { return -1; }
    }

    [UnmanagedCallersOnly(EntryPoint = "get_sensor_value", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static float GetSensorValue(IntPtr ptr)
    {
        try { return Target<ISensor>(ptr)?.Value ?? float.NaN; }
        catch { return float.NaN; }
    }

    [UnmanagedCallersOnly(EntryPoint = "get_sensor_has_control", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static int GetSensorHasControl(IntPtr ptr)
    {
        try { return Target<ISensor>(ptr)?.Control is not null ? 1 : 0; }
        catch { return 0; }
    }

    [UnmanagedCallersOnly(EntryPoint = "get_sensor_control_mode", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static int GetSensorControlMode(IntPtr ptr)
    {
        try { return Target<ISensor>(ptr)?.Control is { } control ? (int)control.ControlMode : -1; }
        catch { return -1; }
    }

    [UnmanagedCallersOnly(EntryPoint = "get_sensor_control_software_value", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static float GetSensorControlSoftwareValue(IntPtr ptr)
    {
        try { return Target<ISensor>(ptr)?.Control is { } control ? control.SoftwareValue : float.NaN; }
        catch { return float.NaN; }
    }

    [UnmanagedCallersOnly(EntryPoint = "set_sensor_software", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static int SetSensorSoftware(IntPtr ptr, float value)
    {
        try
        {
            IControl? control = Target<ISensor>(ptr)?.Control;
            if (control is null)
            {
                return 0;
            }

            value = Math.Clamp(value, control.MinSoftwareValue, control.MaxSoftwareValue);
            control.SetSoftware(value);
            return 1;
        }
        catch
        {
            return 0;
        }
    }

    [UnmanagedCallersOnly(EntryPoint = "set_sensor_default", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static int SetSensorDefault(IntPtr ptr)
    {
        try
        {
            IControl? control = Target<ISensor>(ptr)?.Control;
            if (control is null)
            {
                return 0;
            }

            control.SetDefault();
            return 1;
        }
        catch
        {
            return 0;
        }
    }

    [UnmanagedCallersOnly(EntryPoint = "free_sensor", CallConvs = new[] { typeof(CallConvCdecl) })]
    public static void FreeSensor(IntPtr ptr) => FreeHandle(ptr);

    private static PtrArray ToPtrArray(System.Collections.Generic.IEnumerable<object> values)
    {
        IntPtr[] ptrs = values
            .Where(value => value is not null)
            .Select(value => GCHandle.ToIntPtr(GCHandle.Alloc(value)))
            .ToArray();
        return new PtrArray(ptrs);
    }

    private static T? Target<T>(IntPtr ptr) where T : class
    {
        if (ptr == IntPtr.Zero)
        {
            return null;
        }

        return GCHandle.FromIntPtr(ptr).Target as T;
    }

    private static void FreeHandle(IntPtr ptr)
    {
        if (ptr != IntPtr.Zero)
        {
            GCHandle.FromIntPtr(ptr).Free();
        }
    }
}

internal static class Strings
{
    public static IntPtr Alloc(string? value)
    {
        if (string.IsNullOrEmpty(value))
        {
            return IntPtr.Zero;
        }

        byte[] bytes = Encoding.UTF8.GetBytes(value + "\0");
        IntPtr ptr = Marshal.AllocHGlobal(bytes.Length);
        Marshal.Copy(bytes, 0, ptr, bytes.Length);
        return ptr;
    }

    public static void Free(IntPtr ptr)
    {
        if (ptr != IntPtr.Zero)
        {
            Marshal.FreeHGlobal(ptr);
        }
    }
}

internal sealed class UpdateVisitor : IVisitor
{
    public void VisitComputer(IComputer computer) => computer.Traverse(this);

    public void VisitHardware(IHardware hardware)
    {
        hardware.Update();
        foreach (IHardware subHardware in hardware.SubHardware)
        {
            subHardware.Accept(this);
        }
    }

    public void VisitSensor(ISensor sensor)
    {
    }

    public void VisitParameter(IParameter parameter)
    {
    }
}
