using System.Text.Json.Serialization;
using GitGui.Core.Processes;

namespace GitGui.ProcessHost;

[JsonSerializable(typeof(ProcessRequest))]
internal partial class RequestJson : JsonSerializerContext;
