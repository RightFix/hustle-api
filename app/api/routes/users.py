from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from app.core.database import get_db
from app.core.security import get_current_user
from app.models.user import Follow, User
from app.schemas.user import FollowInfo, UserOut, UserUpdate

router = APIRouter(prefix="/users", tags=["users"])


@router.get("/me", response_model=UserOut)
def read_me(current_user: User = Depends(get_current_user)) -> User:
    return current_user


@router.patch("/me", response_model=UserOut)
def update_me(
    payload: UserUpdate,
    current_user: User = Depends(get_current_user),
    db: Session = Depends(get_db),
) -> User:
    if payload.bio is not None:
        current_user.bio = payload.bio
    db.commit()
    db.refresh(current_user)
    return current_user


@router.get("/{user_id}", response_model=UserOut)
def read_user(user_id: int, db: Session = Depends(get_db)) -> User:
    user = db.get(User, user_id)
    if user is None:
        raise HTTPException(status_code=404, detail="User not found")
    return user


@router.post("/{user_id}/follow", status_code=status.HTTP_204_NO_CONTENT)
def follow_user(
    user_id: int,
    current_user: User = Depends(get_current_user),
    db: Session = Depends(get_db),
) -> None:
    if user_id == current_user.id:
        raise HTTPException(status_code=400, detail="Cannot follow yourself")
    target = db.get(User, user_id)
    if target is None:
        raise HTTPException(status_code=404, detail="User not found")
    already = db.scalar(
        select(Follow).where(
            Follow.follower_id == current_user.id,
            Follow.following_id == user_id,
        )
    )
    if already is None:
        db.add(Follow(follower_id=current_user.id, following_id=user_id))
        db.commit()


@router.delete("/{user_id}/follow", status_code=status.HTTP_204_NO_CONTENT)
def unfollow_user(
    user_id: int,
    current_user: User = Depends(get_current_user),
    db: Session = Depends(get_db),
) -> None:
    follow = db.scalar(
        select(Follow).where(
            Follow.follower_id == current_user.id,
            Follow.following_id == user_id,
        )
    )
    if follow is not None:
        db.delete(follow)
        db.commit()


@router.get("/{user_id}/follow-info", response_model=FollowInfo)
def follow_info(
    user_id: int,
    current_user: User = Depends(get_current_user),
    db: Session = Depends(get_db),
) -> FollowInfo:
    target = db.get(User, user_id)
    if target is None:
        raise HTTPException(status_code=404, detail="User not found")

    followers_count = db.scalar(
        select(func.count()).select_from(Follow).where(Follow.following_id == user_id)
    )
    following_count = db.scalar(
        select(func.count()).select_from(Follow).where(Follow.follower_id == user_id)
    )
    is_following = (
        db.scalar(
            select(Follow).where(
                Follow.follower_id == current_user.id,
                Follow.following_id == user_id,
            )
        )
        is not None
    )
    return FollowInfo(
        followers_count=followers_count or 0,
        following_count=following_count or 0,
        is_following=bool(is_following),
    )
