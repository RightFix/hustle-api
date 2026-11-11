use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

// ----- Rows -----

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: i32,
    pub email: String,
    pub username: String,
    pub hashed_password: String,
    pub bio: Option<String>,
    pub is_admin: bool,
    pub account_type: String,
    pub location: Option<String>,
    pub availability: Option<String>,
    pub expected_rate: Option<String>,
    pub languages: Option<String>,
    pub avatar_file: Option<String>,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub email_messages_enabled: bool,
    pub is_suspended: bool,
    pub suspended_reason: Option<String>,
    pub suspended_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct PostRow {
    pub id: i32,
    pub author_id: i32,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)]
pub struct PasswordResetRow {
    pub id: i32,
    pub user_id: i32,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ApplicantRow {
    pub id: i32,
    pub username: String,
    pub bio: Option<String>,
    pub avatar_file: Option<String>,
    pub message: Option<String>,
    pub applied_at: DateTime<Utc>,
    pub status: String,
}

// ----- Response DTOs -----

#[derive(Debug, Serialize, ToSchema)]
pub struct UserOut {
    pub id: i32,
    pub email: String,
    pub username: String,
    pub bio: Option<String>,
    pub is_admin: bool,
    pub account_type: String,
    /// City / area, free text.
    pub location: Option<String>,
    /// now | open | unavailable
    pub availability: Option<String>,
    /// Free text, e.g. "₦25,000/day".
    pub expected_rate: Option<String>,
    /// Free text, e.g. "English, Igbo, Pidgin".
    pub languages: Option<String>,
    /// Public URL under /media/avatars/... (absent when no avatar is uploaded).
    pub avatar_url: Option<String>,
    /// Whether the user confirmed control of their email address.
    pub email_verified: bool,
    /// Whether Hustle may email the user about new messages.
    pub email_messages_enabled: bool,
    /// Whether the account is suspended by an admin.
    pub is_suspended: bool,
    /// Admin-provided reason for suspension, if any.
    pub suspended_reason: Option<String>,
    pub suspended_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<UserRow> for UserOut {
    fn from(u: UserRow) -> Self {
        Self {
            id: u.id,
            email: u.email,
            username: u.username,
            bio: u.bio,
            is_admin: u.is_admin,
            account_type: u.account_type,
            location: u.location,
            availability: u.availability,
            expected_rate: u.expected_rate,
            languages: u.languages,
            avatar_url: u.avatar_file.map(|f| format!("/media/avatars/{f}")),
            email_verified: u.email_verified_at.is_some(),
            email_messages_enabled: u.email_messages_enabled,
            is_suspended: u.is_suspended,
            suspended_reason: u.suspended_reason,
            suspended_at: u.suspended_at,
            created_at: u.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PostOut {
    pub id: i32,
    pub author_id: i32,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

impl From<PostRow> for PostOut {
    fn from(p: PostRow) -> Self {
        Self {
            id: p.id,
            author_id: p.author_id,
            content: p.content,
            created_at: p.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FollowInfo {
    pub followers_count: i64,
    pub following_count: i64,
    pub is_following: bool,
    /// Current user has blocked this user.
    pub is_blocked_by_me: bool,
    /// This user has blocked the current user.
    pub is_blocked_by_them: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StatsOut {
    pub user_count: i64,
    pub post_count: i64,
    pub gig_count: i64,
    pub conversation_count: i64,
    pub message_count: i64,
    pub suspended_user_count: i64,
    pub unverified_user_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedUsers {
    pub items: Vec<UserOut>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedPosts {
    pub items: Vec<PostOut>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

#[derive(Debug, FromRow)]
pub struct SkillRow {
    pub id: i32,
    pub skill: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SkillOut {
    pub id: i32,
    pub skill: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GigOut {
    pub id: i32,
    pub poster_id: i32,
    pub title: String,
    pub description: String,
    pub collar: String,
    pub location: Option<String>,
    pub pay: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    /// Hired worker id when a gig is in progress/completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hired_worker_id: Option<i32>,
    /// Whether the current viewer already applied (set on gig responses).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied: Option<bool>,
    /// Viewer's application status for this gig (pending/accepted/rejected), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub my_application_status: Option<String>,
    /// Whether the current viewer saved/bookmarked this gig.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved: Option<bool>,
    pub applicant_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedGigs {
    pub items: Vec<GigOut>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApplicantOut {
    pub id: i32,
    pub username: String,
    pub bio: Option<String>,
    /// Public URL under /media/avatars/... (absent when no avatar is uploaded).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub message: Option<String>,
    pub applied_at: DateTime<Utc>,
    /// pending | accepted | rejected
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Token {
    pub access_token: String,
    pub token_type: String,
}

impl Token {
    pub fn bearer(access_token: String) -> Self {
        Self {
            access_token,
            token_type: "bearer".to_owned(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Message {
    pub detail: String,
}

impl Message {
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

// ----- Request DTOs -----

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub email: String,
    pub username: String,
    pub password: String,
    /// "worker" (default) or "hirer".
    #[serde(default)]
    pub account_type: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePostRequest {
    pub content: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAccountRequest {
    pub bio: Option<String>,
    /// "worker" or "hirer"; invalid values are rejected.
    pub account_type: Option<String>,
    /// City / area, free text (empty string clears it).
    pub location: Option<String>,
    /// "now" | "open" | "unavailable" (empty string clears it).
    pub availability: Option<String>,
    /// Free text, e.g. "₦25,000/day".
    pub expected_rate: Option<String>,
    /// Free text, e.g. "English, Igbo, Pidgin".
    pub languages: Option<String>,
    /// Send email notifications for new messages.
    pub email_messages_enabled: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateGigRequest {
    pub title: String,
    pub description: String,
    pub collar: String,
    pub location: Option<String>,
    pub pay: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ApplyGigRequest {
    pub message: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateGigStatusRequest {
    /// "open" or "closed".
    pub status: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateSkillsRequest {
    /// Full replacement list, 1..=20 entries, each 2..=40 chars.
    pub skills: Vec<String>,
}

// ----- Messaging -----

#[derive(Debug, FromRow)]
pub struct MessageRow {
    pub id: i32,
    pub conversation_id: i32,
    pub sender_id: i32,
    pub body: String,
    pub read_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Row shape produced by the conversation-list SQL (user_a/user_b depend on viewer).
#[derive(Debug, FromRow)]
pub struct ConversationListRow {
    pub id: i32,
    pub other_id: i32,
    pub other_username: String,
    pub other_bio: Option<String>,
    pub other_avatar: Option<String>,
    pub last_body: Option<String>,
    pub last_created_at: Option<DateTime<Utc>>,
    pub last_sender_id: Option<i32>,
    pub unread_count: i64,
    pub updated_at: DateTime<Utc>,
    pub is_blocked_by_me: bool,
    pub is_blocked_by_them: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MessageOut {
    pub id: i32,
    pub conversation_id: i32,
    pub sender_id: i32,
    pub body: String,
    pub read_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<MessageRow> for MessageOut {
    fn from(m: MessageRow) -> Self {
        Self {
            id: m.id,
            conversation_id: m.conversation_id,
            sender_id: m.sender_id,
            body: m.body,
            read_at: m.read_at,
            delivered_at: m.delivered_at,
            created_at: m.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserBrief {
    pub id: i32,
    pub username: String,
    pub bio: Option<String>,
    /// Public URL under /media/avatars/... (absent when no avatar is uploaded).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MessageBrief {
    pub body: String,
    pub sender_id: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConversationOut {
    pub id: i32,
    pub other_user: UserBrief,
    pub last_message: Option<MessageBrief>,
    pub unread_count: i64,
    pub updated_at: DateTime<Utc>,
    pub is_blocked_by_me: bool,
    pub is_blocked_by_them: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct StartConversationRequest {
    pub user_id: i32,
}

/// Payload shape for the websocket `send` event (documented for API consumers;
/// sent over WS, not REST, so the field is never read by handlers directly).
#[allow(dead_code)]
#[derive(Debug, Deserialize, ToSchema)]
pub struct SendMessageRequest {
    pub body: String,
}

// ----- Notifications -----

#[derive(Debug, FromRow)]
pub struct NotificationRow {
    pub id: i32,
    pub actor_id: i32,
    pub kind: String,
    pub ref_type: Option<String>,
    pub ref_id: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

/// Row shape from the notifications list query (joins the actor profile).
#[derive(Debug, FromRow)]
pub struct NotificationListRow {
    pub id: i32,
    pub actor_id: i32,
    pub kind: String,
    pub ref_type: Option<String>,
    pub ref_id: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
    pub actor_username: String,
    pub actor_bio: Option<String>,
    pub actor_avatar: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationOut {
    pub id: i32,
    /// One of: follow, gig_apply, message.
    pub kind: String,
    pub actor: UserBrief,
    /// Object kind the notification points at: gig | conversation | null.
    pub ref_type: Option<String>,
    pub ref_id: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationPage {
    pub items: Vec<NotificationOut>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
    pub unread_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UnreadCountOut {
    pub count: i64,
}

// ----- Work gallery -----

#[derive(Debug, FromRow)]
pub struct GalleryItemRow {
    pub id: i32,
    pub user_id: i32,
    pub kind: String,
    pub file_name: String,
    pub mime_type: String,
    pub byte_size: i32,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_seconds: Option<i32>,
    pub caption: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GalleryItemOut {
    pub id: i32,
    pub user_id: i32,
    /// image | video
    pub kind: String,
    /// Public URL under /media/gallery/...
    pub url: String,
    pub mime_type: String,
    pub byte_size: i32,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_seconds: Option<i32>,
    pub caption: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl GalleryItemRow {
    pub fn into_out(self) -> GalleryItemOut {
        GalleryItemOut {
            id: self.id,
            user_id: self.user_id,
            kind: self.kind,
            url: format!("/media/gallery/{}", self.file_name),
            mime_type: self.mime_type,
            byte_size: self.byte_size,
            width: self.width,
            height: self.height,
            duration_seconds: self.duration_seconds,
            caption: self.caption,
            created_at: self.created_at,
        }
    }
}


#[derive(Debug, Deserialize, ToSchema)]
pub struct AdminUpdateUser {
    /// Suspend or unsuspend the account.
    pub is_suspended: Option<bool>,
    /// Reason shown/stored for suspension; cleared when unsuspending unless provided.
    pub suspended_reason: Option<String>,
    /// Manually mark email as verified (true) or clear verification (false).
    pub email_verified: Option<bool>,
    /// Promote/demote admin. Cannot remove your own admin access via the endpoint.
    pub is_admin: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminUserDetail {
    #[serde(flatten)]
    pub user: UserOut,
    pub post_count: i64,
    pub gig_count: i64,
    pub application_count: i64,
    pub conversation_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminPostOut {
    pub id: i32,
    pub author_id: i32,
    pub author_username: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedAdminPosts {
    pub items: Vec<AdminPostOut>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminGigOut {
    pub id: i32,
    pub poster_id: i32,
    pub poster_username: String,
    pub title: String,
    pub description: String,
    pub collar: String,
    pub status: String,
    pub location: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedAdminGigs {
    pub items: Vec<AdminGigOut>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}


// ----- Reports -----

#[derive(Debug, Clone, FromRow)]
pub struct ReportRow {
    pub id: i32,
    pub reporter_id: i32,
    pub target_type: String,
    pub target_id: i32,
    pub reason: String,
    pub details: Option<String>,
    pub status: String,
    pub admin_note: Option<String>,
    pub resolved_by: Option<i32>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct AdminReportRow {
    pub id: i32,
    pub reporter_id: i32,
    pub reporter_username: String,
    pub target_type: String,
    pub target_id: i32,
    pub target_label: Option<String>,
    pub reason: String,
    pub details: Option<String>,
    pub status: String,
    pub admin_note: Option<String>,
    pub resolved_by: Option<i32>,
    pub resolved_by_username: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReportOut {
    pub id: i32,
    pub reporter_id: i32,
    pub target_type: String,
    pub target_id: i32,
    pub reason: String,
    pub details: Option<String>,
    pub status: String,
    pub admin_note: Option<String>,
    pub resolved_by: Option<i32>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<ReportRow> for ReportOut {
    fn from(r: ReportRow) -> Self {
        Self {
            id: r.id,
            reporter_id: r.reporter_id,
            target_type: r.target_type,
            target_id: r.target_id,
            reason: r.reason,
            details: r.details,
            status: r.status,
            admin_note: r.admin_note,
            resolved_by: r.resolved_by,
            resolved_at: r.resolved_at,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminReportOut {
    pub id: i32,
    pub reporter_id: i32,
    pub reporter_username: String,
    pub target_type: String,
    pub target_id: i32,
    /// Human-friendly label when resolvable (username, post excerpt, gig title).
    pub target_label: Option<String>,
    pub reason: String,
    pub details: Option<String>,
    pub status: String,
    pub admin_note: Option<String>,
    pub resolved_by: Option<i32>,
    pub resolved_by_username: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<AdminReportRow> for AdminReportOut {
    fn from(r: AdminReportRow) -> Self {
        Self {
            id: r.id,
            reporter_id: r.reporter_id,
            reporter_username: r.reporter_username,
            target_type: r.target_type,
            target_id: r.target_id,
            target_label: r.target_label,
            reason: r.reason,
            details: r.details,
            status: r.status,
            admin_note: r.admin_note,
            resolved_by: r.resolved_by,
            resolved_by_username: r.resolved_by_username,
            resolved_at: r.resolved_at,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedAdminReports {
    pub items: Vec<AdminReportOut>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateReportRequest {
    /// "user" | "post" | "gig"
    pub target_type: String,
    pub target_id: i32,
    /// Short reason/category (e.g. spam, harassment, scam, inappropriate, other).
    pub reason: String,
    /// Optional additional context.
    pub details: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveReportRequest {
    /// "resolved" | "dismissed"
    pub status: String,
    pub admin_note: Option<String>,
    /// If target is a user and resolving, optionally suspend the user.
    #[serde(default)]
    pub suspend_user: bool,
    /// Reason for suspension when suspend_user is true.
    pub suspend_reason: Option<String>,
    /// If target is a post or gig and resolving, optionally delete the target content.
    #[serde(default)]
    pub delete_target: bool,
    /// If target is a user and resolving, optionally delete the user account (destructive).
    #[serde(default)]
    pub delete_user: bool,
}


// ----- Blocks -----

#[derive(Debug, Serialize, ToSchema)]
pub struct BlockedUserOut {
    pub id: i32,
    pub username: String,
    pub bio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub blocked_at: DateTime<Utc>,
}


// ----- Admin audit -----

#[derive(Debug, Clone, FromRow)]
pub struct AdminAuditRow {
    pub id: i32,
    pub actor_id: Option<i32>,
    pub actor_username: Option<String>,
    pub action: String,
    pub target_type: String,
    pub target_id: Option<i32>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminAuditOut {
    pub id: i32,
    pub actor_id: Option<i32>,
    pub actor_username: Option<String>,
    pub action: String,
    pub target_type: String,
    pub target_id: Option<i32>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl From<AdminAuditRow> for AdminAuditOut {
    fn from(r: AdminAuditRow) -> Self {
        Self {
            id: r.id,
            actor_id: r.actor_id,
            actor_username: r.actor_username,
            action: r.action,
            target_type: r.target_type,
            target_id: r.target_id,
            metadata: r.metadata,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedAdminAudit {
    pub items: Vec<AdminAuditOut>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}


// ----- Ratings (Round 4) -----

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRatingRequest {
    /// 1-5
    pub rating: i16,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct RatingRow {
    pub id: i32,
    pub gig_id: i32,
    pub rater_id: i32,
    pub rater_username: String,
    pub rater_avatar_file: Option<String>,
    pub ratee_id: i32,
    pub rating: i16,
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RatingOut {
    pub id: i32,
    pub gig_id: i32,
    pub rater_id: i32,
    pub rater_username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rater_avatar_url: Option<String>,
    pub ratee_id: i32,
    pub rating: i16,
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<RatingRow> for RatingOut {
    fn from(r: RatingRow) -> Self {
        Self {
            id: r.id,
            gig_id: r.gig_id,
            rater_id: r.rater_id,
            rater_username: r.rater_username,
            rater_avatar_url: r.rater_avatar_file.map(|f| format!("/media/avatars/{f}")),
            ratee_id: r.ratee_id,
            rating: r.rating,
            comment: r.comment,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RatingSummary {
    pub average: Option<f64>,
    pub count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserRatingsOut {
    pub summary: RatingSummary,
    pub items: Vec<RatingOut>,
}
