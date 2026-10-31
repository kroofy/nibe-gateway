use super::*;
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioRequest {
    pub name: String,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdvanceRequest {
    pub milliseconds: u64,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FreezeRequest {
    pub frozen: bool,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FaultRequest {
    pub kind: String,
}
async fn apply(state: ApiState, action: DevAction) -> Result<Json<DeviceSnapshot>, ApiError> {
    state.gateway.dev_action(action).await?;
    Ok(Json(state.gateway.snapshot()))
}
#[utoipa::path(post, path = "/__dev/scenario", request_body = ScenarioRequest, responses((status = 200, body = DeviceSnapshot), (status = 422, body = ApiErrorBody)))]
pub async fn dev_scenario(
    State(state): State<ApiState>,
    json: Result<Json<ScenarioRequest>, JsonRejection>,
) -> Result<Json<DeviceSnapshot>, ApiError> {
    apply(
        state,
        DevAction::Scenario {
            name: body(json)?.name,
        },
    )
    .await
}
#[utoipa::path(post, path = "/__dev/time/advance", request_body = AdvanceRequest, responses((status = 200, body = DeviceSnapshot), (status = 422, body = ApiErrorBody)))]
pub async fn dev_advance(
    State(state): State<ApiState>,
    json: Result<Json<AdvanceRequest>, JsonRejection>,
) -> Result<Json<DeviceSnapshot>, ApiError> {
    apply(
        state,
        DevAction::Advance {
            milliseconds: body(json)?.milliseconds,
        },
    )
    .await
}
#[utoipa::path(post, path = "/__dev/time/freeze", request_body = FreezeRequest, responses((status = 200, body = DeviceSnapshot)))]
pub async fn dev_freeze(
    State(state): State<ApiState>,
    json: Result<Json<FreezeRequest>, JsonRejection>,
) -> Result<Json<DeviceSnapshot>, ApiError> {
    apply(
        state,
        DevAction::Freeze {
            frozen: body(json)?.frozen,
        },
    )
    .await
}
#[utoipa::path(post, path = "/__dev/fault", request_body = FaultRequest, responses((status = 200, body = DeviceSnapshot), (status = 422, body = ApiErrorBody)))]
pub async fn dev_fault(
    State(state): State<ApiState>,
    json: Result<Json<FaultRequest>, JsonRejection>,
) -> Result<Json<DeviceSnapshot>, ApiError> {
    apply(
        state,
        DevAction::Fault {
            kind: body(json)?.kind,
        },
    )
    .await
}
#[utoipa::path(post, path = "/__dev/reset", responses((status = 200, body = DeviceSnapshot)))]
pub async fn dev_reset(State(state): State<ApiState>) -> Result<Json<DeviceSnapshot>, ApiError> {
    apply(state, DevAction::Reset).await
}
#[utoipa::path(get, path = "/__dev/commands", responses((status = 200, body = [CommandReceipt])))]
pub async fn dev_commands(State(state): State<ApiState>) -> Json<Vec<CommandReceipt>> {
    Json(state.gateway.command_history())
}
#[derive(OpenApi)]
#[openapi(paths(
    dev_scenario,
    dev_advance,
    dev_freeze,
    dev_fault,
    dev_reset,
    dev_commands
))]
pub struct DevDocument;
